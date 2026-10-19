package control

import (
	"bytes"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

const adminToken = "super-secret-lab-admin-12345678"
const agentToken = "super-secret-node-token-12345678"

func apiTest(t *testing.T) (*Controller, http.Handler) {
	t.Helper()
	c, _ := testControl(t, "")
	h := (API{Control: c, AdminToken: adminToken, AgentTokens: map[string]string{"node-a": agentToken, "node-b": agentToken + "b"}}).Handler()
	return c, h
}
func httpCall(h http.Handler, method, path, token, key string, obj any) *httptest.ResponseRecorder {
	var b []byte
	if obj != nil {
		b, _ = json.Marshal(obj)
	}
	req := httptest.NewRequest(method, path, bytes.NewReader(b))
	if obj != nil {
		req.Header.Set("Content-Type", "application/json")
	}
	if token != "" {
		req.Header.Set("Authorization", "Bearer "+token)
	}
	if key != "" {
		req.Header.Set("Idempotency-Key", key)
	}
	w := httptest.NewRecorder()
	h.ServeHTTP(w, req)
	return w
}
func TestHTTPAuthAndAllocation(t *testing.T) {
	_, h := apiTest(t)
	if resp := httpCall(h, "GET", "/v1/nodes", "", "", nil); resp.Code != 401 {
		t.Fatal(resp.Code)
	}
	if resp := httpCall(h, "GET", "/healthz", "", "", nil); resp.Code != 200 {
		t.Fatal(resp.Code)
	}
	sample := goodTelemetry()
	if resp := httpCall(h, "POST", "/internal/nodes/node-a/heartbeat", agentToken+"wrong", "", sample); resp.Code != 401 {
		t.Fatal(resp.Code)
	}
	if resp := httpCall(h, "POST", "/internal/nodes/node-a/heartbeat", agentToken, "", sample); resp.Code != 204 {
		t.Fatalf("%d %s", resp.Code, resp.Body.String())
	}
	if resp := httpCall(h, "GET", "/v1/nodes", adminToken, "", nil); resp.Code != 200 || !strings.Contains(resp.Body.String(), "node-a") {
		t.Fatal(resp.Code)
	}
	key := "request-key-123456789"
	res := httpCall(h, "POST", "/v1/reservations", adminToken, key, request("room-1"))
	if res.Code != 201 {
		t.Fatalf("got %d %s", res.Code, res.Body.String())
	}
	var record struct {
		Reservation        Allocation `json:"reservation"`
		GameServerLaunched bool       `json:"gameServerLaunched"`
	}
	if err := json.Unmarshal(res.Body.Bytes(), &record); err != nil {
		t.Fatal(err)
	}
	if record.GameServerLaunched || record.Reservation.NodeID != "node-a" {
		t.Fatal("fake deployment or wrong node")
	}
	if res := httpCall(h, "POST", "/v1/reservations", adminToken, key, request("room-1")); res.Code != 200 {
		t.Fatalf("retry got %d", res.Code)
	}
	changed := request("room-2")
	if res := httpCall(h, "POST", "/v1/reservations", adminToken, key, changed); res.Code != 409 {
		t.Fatalf("conflict got %d", res.Code)
	}
	release := httpCall(h, "POST", fmt.Sprintf("/v1/reservations/%s/release", record.Reservation.ID), adminToken, "", nil)
	if release.Code != 200 {
		t.Fatal(release.Code)
	}
}
func TestHTTPIsStrictAndPrivate(t *testing.T) {
	_, h := apiTest(t)
	heartbeat := httpCall(h, "POST", "/internal/nodes/node-a/heartbeat", adminToken, "", goodTelemetry())
	if heartbeat.Code != 401 {
		t.Fatal("admin key cannot impersonate node")
	}
	tooLarge := strings.Repeat("a", 4200)
	req := httptest.NewRequest("POST", "/v1/reservations", strings.NewReader(tooLarge))
	req.Header.Set("Authorization", "Bearer "+adminToken)
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	h.ServeHTTP(w, req)
	if w.Code != 413 {
		t.Fatalf("expected 413 got %d", w.Code)
	}
	if got := w.Header().Get("X-Content-Type-Options"); got != "nosniff" {
		t.Fatal(got)
	}
	req = httptest.NewRequest("POST", "/v1/reservations", strings.NewReader(`{"roomId":"room-1","region":"us-test","cpuMilli":1000,"memoryMiB":1000,"ttlSeconds":300,"extra":true}`))
	req.Header.Set("Authorization", "Bearer "+adminToken)
	req.Header.Set("Content-Type", "application/json")
	w = httptest.NewRecorder()
	h.ServeHTTP(w, req)
	if w.Code != 400 {
		t.Fatal("unexpected fields must reject")
	}
}
