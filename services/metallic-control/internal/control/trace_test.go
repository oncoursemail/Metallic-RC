package control

import (
	"net/http"
	"net/http/httptest"
	"regexp"
	"testing"
)

var traceIDPattern = regexp.MustCompile(`^[0-9a-f]{32}$`)

func traceCall(h http.Handler, path, traceparent string) *httptest.ResponseRecorder {
	req := httptest.NewRequest("GET", path, nil)
	if traceparent != "" {
		req.Header.Set("Traceparent", traceparent)
	}
	w := httptest.NewRecorder()
	h.ServeHTTP(w, req)
	return w
}

func TestTraceIDGeneratedWhenMissing(t *testing.T) {
	_, h := apiTest(t)
	a := traceCall(h, "/healthz", "").Header().Get("X-Trace-Id")
	b := traceCall(h, "/healthz", "").Header().Get("X-Trace-Id")
	if !traceIDPattern.MatchString(a) || a == b {
		t.Fatalf("want unique 32-hex trace ids, got %q %q", a, b)
	}
}

func TestTraceIDPropagatedFromTraceparent(t *testing.T) {
	_, h := apiTest(t)
	id := "4bf92f3577b34da6a3ce929d0e0e4736"
	if got := traceCall(h, "/healthz", "00-"+id+"-00f067aa0ba902b7-01").Header().Get("X-Trace-Id"); got != id {
		t.Fatalf("got %q", got)
	}
}

func TestTraceIDRejectsMalformedAndZeroTraceparent(t *testing.T) {
	_, h := apiTest(t)
	for _, tp := range []string{"garbage", "00-00000000000000000000000000000000-00f067aa0ba902b7-01", "00-4BF92F3577B34DA6A3CE929D0E0E4736-00f067aa0ba902b7-01"} {
		got := traceCall(h, "/healthz", tp).Header().Get("X-Trace-Id")
		if !traceIDPattern.MatchString(got) || got == "00000000000000000000000000000000" || got == "4bf92f3577b34da6a3ce929d0e0e4736" {
			t.Fatalf("%q -> %q", tp, got)
		}
	}
}

func TestTraceIDOnUnauthorizedResponse(t *testing.T) {
	_, h := apiTest(t)
	if traceCall(h, "/v1/nodes", "").Header().Get("X-Trace-Id") == "" {
		t.Fatal("401 response lacks trace id")
	}
}
