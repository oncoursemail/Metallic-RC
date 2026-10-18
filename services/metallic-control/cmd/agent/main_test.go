package main

import (
	"context"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/oncoursemail/Metallic/services/metallic-control/internal/control"
)

func TestOnlyAllowHTTPSOrLoopback(t *testing.T) {
	tests := []struct {
		origin  string
		allowed bool
	}{
		{"http://127.0.0.1:8098", true},
		{"http://[::1]:8098", true},
		{"https://control.metallic.invalid", true},
		{"http://198.51.100.1:8098", false},
		{"http://control.metallic.invalid", false},
		{"https://user:pass@control.invalid", false},
		{"https://control.invalid/hidden", false},
	}
	for _, tt := range tests {
		t.Run(tt.origin, func(t *testing.T) {
			_, err := checkedEndpoint(tt.origin, "node-test")
			if (err == nil) != tt.allowed {
				t.Fatalf("allowed %v err %v", tt.allowed, err)
			}
		})
	}
}
func TestCollectorDoesNotNeedPrivileges(t *testing.T) {
	met, err := telemetry()
	if err != nil {
		t.Fatal(err)
	}
	if met.AvailableMemoryMiB <= 0 || met.FreeDiskMiB < 0 || met.Load1 < 0 {
		t.Fatalf("bad metrics: %+v", met)
	}
}
func TestAgentSendsWithoutExposingToken(t *testing.T) {
	token := "private-agent-token-placeholder"
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Authorization") != "Bearer "+token {
			t.Error("missing bearer")
		}
		if r.Header.Get("Content-Type") != "application/json" {
			t.Error("bad content type")
		}
		w.WriteHeader(http.StatusNoContent)
	}))
	defer server.Close()
	err := send(context.Background(), server.Client(), server.URL, token, control.Telemetry{AvailableMemoryMiB: 500, FreeDiskMiB: 200, Load1: 1})
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(errString(err), token) {
		t.Fatal("secret in error")
	}
}
func errString(e error) string {
	if e == nil {
		return ""
	}
	return e.Error()
}
