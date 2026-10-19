// Linux, unprivileged, read-only agent for the local Metallic lab.
// The agent never runs arbitrary commands from the control plane.
package main

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"net/url"
	"os"
	"os/signal"
	"strconv"
	"strings"
	"syscall"
	"time"

	"github.com/oncoursemail/Metallic/services/metallic-control/internal/control"
)

func readMem() (map[string]int, error) {
	b, e := os.ReadFile("/proc/meminfo")
	if e != nil {
		return nil, e
	}
	vals := map[string]int{}
	for _, line := range strings.Split(string(b), "\n") {
		f := strings.Fields(line)
		if len(f) >= 2 {
			if v, err := strconv.Atoi(f[1]); err == nil {
				vals[strings.TrimSuffix(f[0], ":")] = v / 1024
			}
		}
	}
	for _, key := range []string{"MemAvailable", "SwapTotal", "SwapFree"} {
		if _, ok := vals[key]; !ok {
			return nil, fmt.Errorf("missing %s", key)
		}
	}
	return vals, nil
}
func telemetry() (control.Telemetry, error) {
	m, e := readMem()
	if e != nil {
		return control.Telemetry{}, e
	}
	b, e := os.ReadFile("/proc/loadavg")
	if e != nil {
		return control.Telemetry{}, e
	}
	f := strings.Fields(string(b))
	if len(f) == 0 {
		return control.Telemetry{}, errors.New("invalid loadavg")
	}
	load, e := strconv.ParseFloat(f[0], 64)
	if e != nil {
		return control.Telemetry{}, e
	}
	var stat syscall.Statfs_t
	if e = syscall.Statfs("/", &stat); e != nil {
		return control.Telemetry{}, e
	}
	return control.Telemetry{
		AvailableMemoryMiB: m["MemAvailable"], FreeDiskMiB: int(stat.Bavail * uint64(stat.Bsize) / 1048576), Load1: load,
		SwapUsedMiB: m["SwapTotal"] - m["SwapFree"], SwapTotalMiB: m["SwapTotal"],
	}, nil
}
func checkedEndpoint(raw, id string) (string, error) {
	u, e := url.Parse(raw)
	if e != nil {
		return "", e
	}
	if u.User != nil || u.RawQuery != "" || u.Fragment != "" || u.Host == "" || u.Path != "" && u.Path != "/" {
		return "", errors.New("base URL must be origin without credentials, query or path")
	}
	if u.Scheme != "https" {
		host := u.Hostname()
		ip := net.ParseIP(host)
		if u.Scheme != "http" || !(ip != nil && ip.IsLoopback() || host == "localhost") {
			return "", errors.New("remote telemetry requires HTTPS")
		}
	}
	if id == "" {
		return "", errors.New("node ID required")
	}
	for _, r := range id {
		if !((r >= 'a' && r <= 'z') || (r >= 'A' && r <= 'Z') || (r >= '0' && r <= '9') || r == '_' || r == '-') {
			return "", errors.New("invalid node ID")
		}
	}
	return strings.TrimSuffix(raw, "/") + "/internal/nodes/" + id + "/heartbeat", nil
}
func send(ctx context.Context, client *http.Client, endpoint, token string, t control.Telemetry) error {
	b, e := json.Marshal(t)
	if e != nil {
		return e
	}
	req, e := http.NewRequestWithContext(ctx, http.MethodPost, endpoint, bytes.NewReader(b))
	if e != nil {
		return e
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", "Bearer "+token)
	// Endpoint validation applies to this URL only. Even a same-host redirect
	// can downgrade HTTPS or send the bearer and telemetry to another service.
	// Copy the client to preserve its transport/timeout without mutating a
	// shared client while another heartbeat could be in flight.
	boundClient := *client
	boundClient.CheckRedirect = func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }
	res, e := boundClient.Do(req)
	if e != nil {
		return e
	}
	defer res.Body.Close()
	_, _ = io.Copy(io.Discard, io.LimitReader(res.Body, 4096))
	if res.StatusCode != http.StatusNoContent {
		return fmt.Errorf("heartbeat not accepted: HTTP %d", res.StatusCode)
	}
	return nil
}
func main() {
	node := os.Getenv("METALLIC_NODE_ID")
	token := os.Getenv("METALLIC_NODE_TOKEN")
	base := os.Getenv("METALLIC_API_ORIGIN")
	if len(token) < 24 {
		log.Fatal("missing METALLIC_NODE_TOKEN (>=24 characters required)")
	}
	endpoint, e := checkedEndpoint(base, node)
	if e != nil {
		log.Fatalf("bad endpoint: %v", e)
	}
	ctx, cancel := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer cancel()
	client := &http.Client{Timeout: 5 * time.Second}
	ticker := time.NewTicker(8 * time.Second)
	defer ticker.Stop()
	for {
		t, e := telemetry()
		if e == nil {
			e = send(ctx, client, endpoint, token, t)
		}
		if e != nil {
			log.Printf("heartbeat failed (no secrets): %v", e)
		}
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
		}
	}
}
