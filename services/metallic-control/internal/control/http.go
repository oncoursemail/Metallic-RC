package control

import (
	"bytes"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/json"
	"errors"
	"io"
	"mime"
	"net/http"
	"strconv"
	"strings"
	"unicode/utf8"
)

// API is an administrator/agent-only lab API; it is NOT public player login.
type API struct {
	Control     *Controller
	AdminToken  string
	AgentTokens map[string]string
}

func equalSecret(candidate, expected string) bool {
	a := sha256.Sum256([]byte(candidate))
	b := sha256.Sum256([]byte(expected))
	return expected != "" && subtle.ConstantTimeCompare(a[:], b[:]) == 1
}
func bearer(r *http.Request) string {
	h := r.Header.Values("Authorization")
	if len(h) != 1 || !strings.HasPrefix(h[0], "Bearer ") {
		return ""
	}
	return strings.TrimPrefix(h[0], "Bearer ")
}
func jsonReply(w http.ResponseWriter, status int, obj any) {
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	w.Header().Set("Cache-Control", "no-store")
	w.Header().Set("X-Content-Type-Options", "nosniff")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(obj)
}
func apiError(w http.ResponseWriter, err error) {
	status, code := 500, "internal_error"
	switch {
	case errors.Is(err, ErrTooLarge):
		status, code = 413, "payload_too_large"
	case errors.Is(err, ErrInvalid):
		status, code = 400, "invalid_input"
	case errors.Is(err, ErrConflict):
		status, code = 409, "conflict"
	case errors.Is(err, ErrNoCapacity):
		status, code = 503, "no_capacity"
	case errors.Is(err, ErrNotFound):
		status, code = 404, "not_found"
	}
	jsonReply(w, status, map[string]string{"error": code})
}

// Every current request is a flat object with required, non-null fields. Check
// its shape before decoding values: encoding/json alone permits missing fields,
// null scalars, duplicate names and case aliases that can hide bad telemetry.
func readJSON(w http.ResponseWriter, r *http.Request, out any, fields ...string) error {
	ct := r.Header.Get("Content-Type")
	mediatype, _, err := mime.ParseMediaType(ct)
	if err != nil || mediatype != "application/json" {
		return ErrInvalid
	}
	body, err := io.ReadAll(http.MaxBytesReader(w, r.Body, 4096))
	if err != nil {
		var max *http.MaxBytesError
		if errors.As(err, &max) {
			return ErrTooLarge
		}
		return ErrInvalid
	}
	if !utf8.Valid(body) {
		return ErrInvalid
	}
	dec := json.NewDecoder(bytes.NewReader(body))
	tok, err := dec.Token()
	if err != nil || tok != json.Delim('{') {
		return ErrInvalid
	}
	seen := make(map[string]bool, len(fields))
	for _, field := range fields {
		seen[field] = false
	}
	for dec.More() {
		tok, err = dec.Token()
		key, ok := tok.(string)
		if err != nil || !ok {
			return ErrInvalid
		}
		if present, allowed := seen[key]; !allowed || present {
			return ErrInvalid
		}
		var value json.RawMessage
		if err := dec.Decode(&value); err != nil || bytes.Equal(bytes.TrimSpace(value), []byte("null")) {
			return ErrInvalid
		}
		seen[key] = true
	}
	if tok, err = dec.Token(); err != nil || tok != json.Delim('}') {
		return ErrInvalid
	}
	if _, err = dec.Token(); err != io.EOF {
		return ErrInvalid
	}
	for _, present := range seen {
		if !present {
			return ErrInvalid
		}
	}
	if err := json.Unmarshal(body, out); err != nil {
		return ErrInvalid
	}
	return nil
}
func (a API) admin(next http.HandlerFunc) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		if !equalSecret(bearer(r), a.AdminToken) {
			jsonReply(w, 401, map[string]string{"error": "unauthorized"})
			return
		}
		next(w, r)
	}
}
func (a API) Handler() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("GET /healthz", func(w http.ResponseWriter, r *http.Request) {
		jsonReply(w, 200, map[string]any{"status": "alive", "labOnly": true})
	})
	mux.HandleFunc("POST /internal/nodes/{nodeID}/heartbeat", func(w http.ResponseWriter, r *http.Request) {
		id := r.PathValue("nodeID")
		token, ok := a.AgentTokens[id]
		if !ok || !equalSecret(bearer(r), token) {
			jsonReply(w, 401, map[string]string{"error": "unauthorized"})
			return
		}
		var h Telemetry
		if err := readJSON(w, r, &h, "availableMemoryMiB", "freeDiskMiB", "load1", "swapUsedMiB", "swapTotalMiB"); err != nil {
			apiError(w, err)
			return
		}
		if err := a.Control.Heartbeat(id, h); err != nil {
			apiError(w, err)
			return
		}
		w.Header().Set("Cache-Control", "no-store")
		w.WriteHeader(http.StatusNoContent)
	})
	mux.HandleFunc("GET /v1/nodes", a.admin(func(w http.ResponseWriter, r *http.Request) {
		jsonReply(w, 200, map[string]any{"labOnly": true, "nodes": a.Control.Nodes()})
	}))
	mux.HandleFunc("GET /v1/reservations", a.admin(func(w http.ResponseWriter, r *http.Request) {
		jsonReply(w, 200, map[string]any{"labOnly": true, "reservations": a.Control.Allocations()})
	}))
	mux.HandleFunc("POST /v1/reservations", a.admin(func(w http.ResponseWriter, r *http.Request) {
		var req Request
		if err := readJSON(w, r, &req, "roomId", "region", "cpuMilli", "memoryMiB", "ttlSeconds"); err != nil {
			apiError(w, err)
			return
		}
		alloc, existing, err := a.Control.Reserve(req, r.Header.Get("Idempotency-Key"))
		if err != nil {
			apiError(w, err)
			return
		}
		status := http.StatusCreated
		if existing {
			status = http.StatusOK
		}
		jsonReply(w, status, map[string]any{"labOnly": true, "reservation": alloc, "wasExisting": existing, "gameServerLaunched": false})
	}))
	mux.HandleFunc("POST /v1/reservations/{reservationID}/release", a.admin(func(w http.ResponseWriter, r *http.Request) {
		if _, err := a.Control.Release(r.PathValue("reservationID")); err != nil {
			apiError(w, err)
			return
		}
		jsonReply(w, 200, map[string]any{"released": true, "labOnly": true})
	}))
	mux.HandleFunc("POST /v1/reservations/{reservationID}/renew", a.admin(func(w http.ResponseWriter, r *http.Request) {
		var obj struct {
			TTLSeconds int `json:"ttlSeconds"`
		}
		if err := readJSON(w, r, &obj, "ttlSeconds"); err != nil {
			apiError(w, err)
			return
		}
		alloc, err := a.Control.Renew(r.PathValue("reservationID"), obj.TTLSeconds)
		if err != nil {
			apiError(w, err)
			return
		}
		jsonReply(w, 200, map[string]any{"reservation": alloc, "labOnly": true})
	}))
	mux.HandleFunc("GET /v1/metrics", a.admin(func(w http.ResponseWriter, r *http.Request) {
		nodes := a.Control.Nodes()
		counts := map[string]int{}
		for _, n := range nodes {
			counts[n.State]++
		}
		jsonReply(w, 200, map[string]any{"labOnly": true, "nodesByState": counts, "activeReservations": len(a.Control.Allocations())})
	}))
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("X-Content-Type-Options", "nosniff")
		w.Header().Set("Cache-Control", "no-store")
		w.Header().Set("X-Frame-Options", "DENY")
		if r.ContentLength > 4096 {
			jsonReply(w, http.StatusRequestEntityTooLarge, map[string]string{"error": "payload_too_large"})
			return
		}
		mux.ServeHTTP(w, r)
	})
}

// For an operator diagnostic endpoint, return a string rather than a secret.
func (a API) String() string {
	return "Metallic Lab Control API (nodes=" + strconv.Itoa(len(a.AgentTokens)) + ")"
}
