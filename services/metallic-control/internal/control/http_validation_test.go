package control

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

func rawJSONCall(h http.Handler, path, token, key, body string) *httptest.ResponseRecorder {
	r := httptest.NewRequest(http.MethodPost, path, strings.NewReader(body))
	r.Header.Set("Content-Type", "application/json")
	r.Header.Set("Authorization", "Bearer "+token)
	if key != "" {
		r.Header.Set("Idempotency-Key", key)
	}
	w := httptest.NewRecorder()
	h.ServeHTTP(w, r)
	return w
}

func TestIncompleteTelemetryCannotRefreshAStaleNode(t *testing.T) {
	for _, field := range []string{"availableMemoryMiB", "freeDiskMiB", "load1", "swapUsedMiB", "swapTotalMiB"} {
		for _, form := range []string{"missing", "null"} {
			t.Run(field+"/"+form, func(t *testing.T) {
				c, now := testControl(t, "")
				if err := c.Heartbeat("node-a", goodTelemetry()); err != nil {
					t.Fatal(err)
				}
				*now = now.Add(21 * time.Second)
				h := (API{Control: c, AdminToken: adminToken, AgentTokens: map[string]string{"node-a": agentToken}}).Handler()
				data, _ := json.Marshal(goodTelemetry())
				var obj map[string]any
				if err := json.Unmarshal(data, &obj); err != nil {
					t.Fatal(err)
				}
				if form == "missing" {
					delete(obj, field)
				} else {
					obj[field] = nil
				}
				body, _ := json.Marshal(obj)
				resp := rawJSONCall(h, "/internal/nodes/node-a/heartbeat", agentToken, "", string(body))
				if resp.Code != http.StatusBadRequest {
					t.Errorf("incomplete telemetry accepted: HTTP %d", resp.Code)
				}
				if state := c.Nodes()[0].State; state != "stale" {
					t.Errorf("invalid heartbeat changed node state to %q", state)
				}
				if _, _, err := c.Reserve(request("room-incomplete"), "incomplete-telemetry-key"); err != ErrNoCapacity {
					t.Errorf("invalid telemetry admitted a reservation: %v", err)
				}
			})
		}
	}
}

func TestHeartbeatRejectsAmbiguousJSON(t *testing.T) {
	good := `"availableMemoryMiB":9000,"freeDiskMiB":30000,"swapUsedMiB":10,"swapTotalMiB":100`
	for name, body := range map[string]string{
		"duplicate":   `{` + good + `,"load1":8,"load1":0}`,
		"case_alias":  `{` + good + `,"Load1":0}`,
		"alias_pair":  `{` + good + `,"load1":8,"LOAD1":0}`,
		"escaped_key": `{` + good + `,"load1":8,"load\u0031":0}`,
		"null_object": `null`,
		"trailing":    `{` + good + `,"load1":0} {}`,
	} {
		t.Run(name, func(t *testing.T) {
			c, h := apiTest(t)
			resp := rawJSONCall(h, "/internal/nodes/node-a/heartbeat", agentToken, "", body)
			if resp.Code != http.StatusBadRequest {
				t.Fatalf("ambiguous telemetry accepted: HTTP %d", resp.Code)
			}
			if c.Nodes()[0].State != "offline" {
				t.Fatal("rejected telemetry refreshed the node")
			}
		})
	}
}

func TestReservationAndRenewRejectDuplicateFields(t *testing.T) {
	c, h := apiTest(t)
	beat(t, c)
	body := `{"roomId":"same-room","region":"us-test","cpuMilli":9000,"cpuMilli":1000,"memoryMiB":1000,"ttlSeconds":300}`
	resp := rawJSONCall(h, "/v1/reservations", adminToken, "duplicate-reservation-key", body)
	if resp.Code != http.StatusBadRequest || len(c.Allocations()) != 0 {
		t.Fatalf("ambiguous reservation changed capacity: HTTP %d, allocations %d", resp.Code, len(c.Allocations()))
	}
	a, _, err := c.Reserve(request("renew-test"), "renew-initial-reservation")
	if err != nil {
		t.Fatal(err)
	}
	resp = rawJSONCall(h, "/v1/reservations/"+a.ID+"/renew", adminToken, "", `{"ttlSeconds":3600,"ttlSeconds":30}`)
	if resp.Code != http.StatusBadRequest {
		t.Fatalf("ambiguous renewal accepted: HTTP %d", resp.Code)
	}
	if !c.Allocations()[0].ExpiresAt.Equal(a.ExpiresAt) {
		t.Fatal("rejected renewal changed the lease")
	}
}
