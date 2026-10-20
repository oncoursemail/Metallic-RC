package main

import (
	"context"
	"fmt"
	"io"
	"net/http"
	"strings"
	"testing"

	"github.com/oncoursemail/Metallic-RC/services/metallic-control/internal/control"
)

type roundTripFunc func(*http.Request) (*http.Response, error)

func (f roundTripFunc) RoundTrip(r *http.Request) (*http.Response, error) { return f(r) }

func TestAgentNeverFollowsTelemetryRedirects(t *testing.T) {
	for _, status := range []int{301, 302, 303, 307, 308} {
		for _, target := range []string{"http://control.metallic.invalid/other", "https://control.metallic.invalid/other", "https://elsewhere.invalid/other"} {
			t.Run(fmt.Sprintf("%d/%s", status, target), func(t *testing.T) {
				// The transport is local test code: no DNS, sockets, or real token.
				token := "fictional-test-node-bearer-only"
				calls := 0
				client := &http.Client{Transport: roundTripFunc(func(r *http.Request) (*http.Response, error) {
					calls++
					resp := &http.Response{StatusCode: http.StatusNoContent, Header: make(http.Header), Body: io.NopCloser(strings.NewReader("")), Request: r}
					if calls == 1 {
						resp.StatusCode = status
						resp.Header.Set("Location", target)
					}
					return resp, nil
				})}
				endpoint, err := checkedEndpoint("https://control.metallic.invalid", "test-node")
				if err != nil {
					t.Fatal(err)
				}
				err = send(context.Background(), client, endpoint, token, control.Telemetry{AvailableMemoryMiB: 500, FreeDiskMiB: 200, Load1: 1})
				if calls != 1 {
					t.Errorf("agent followed redirect: %d requests", calls)
				}
				if err == nil || strings.Contains(err.Error(), token) {
					t.Errorf("redirect must fail with a non-sensitive error: %v", err)
				}
				if client.CheckRedirect != nil {
					t.Error("send changed the caller's reusable HTTP client")
				}
			})
		}
	}
}
