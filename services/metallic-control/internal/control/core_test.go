package control

import (
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"testing"
	"time"
)

func testInv() Inventory {
	return Inventory{Nodes: []NodeSpec{
		{ID: "node-a", Region: "us-test", LogicalCPUs: 8, CPUMilli: 3000, MemoryMiB: 3072, MaxRooms: 2, MinFreeMemoryMiB: 1024, MinFreeDiskMiB: 10000, MaxLoadPerThread: 0.80, MaxSwapUsedFraction: 0.7},
		{ID: "node-b", Region: "us-test", LogicalCPUs: 8, CPUMilli: 3000, MemoryMiB: 3072, MaxRooms: 2, MinFreeMemoryMiB: 1024, MinFreeDiskMiB: 10000, MaxLoadPerThread: 0.80, MaxSwapUsedFraction: 0.7},
	}}
}
func goodTelemetry() Telemetry {
	return Telemetry{AvailableMemoryMiB: 9000, FreeDiskMiB: 30000, Load1: 1.0, SwapTotalMiB: 100, SwapUsedMiB: 10}
}
func request(room string) Request {
	return Request{RoomID: room, Region: "us-test", CPUMilli: 1000, MemoryMiB: 1000, TTLSeconds: 300}
}
func testControl(t *testing.T, path string) (*Controller, *time.Time) {
	t.Helper()
	now := time.Date(2026, 10, 18, 17, 0, 0, 0, time.UTC)
	c, err := New(testInv(), path, func() time.Time { return now })
	if err != nil {
		t.Fatal(err)
	}
	return c, &now
}
func beat(t *testing.T, c *Controller) {
	t.Helper()
	for _, n := range []string{"node-a", "node-b"} {
		if err := c.Heartbeat(n, goodTelemetry()); err != nil {
			t.Fatal(err)
		}
	}
}
func TestNoSessionWithoutFreshHeartbeat(t *testing.T) {
	c, _ := testControl(t, "")
	_, _, err := c.Reserve(request("room-1"), "key-at-least-123456789")
	if err != ErrNoCapacity {
		t.Fatalf("%v", err)
	}
}
func TestExpiryAndNoGhostCapacity(t *testing.T) {
	c, now := testControl(t, "")
	beat(t, c)
	a, _, err := c.Reserve(request("room-1"), "key-at-least-123456789")
	if err != nil {
		t.Fatal(err)
	}
	*now = now.Add(400 * time.Second)
	if len(c.Allocations()) != 0 {
		t.Fatal("expired room counted as active")
	}
	if err = c.Heartbeat("node-a", goodTelemetry()); err != nil {
		t.Fatal(err)
	}
	b, _, err := c.Reserve(request("room-1"), "key-at-least-123456789")
	if err != nil {
		t.Fatal(err)
	}
	if a.ID == b.ID {
		t.Fatal("expired id reused")
	}
}
func TestIdempotencyConflictsAndRelease(t *testing.T) {
	c, _ := testControl(t, "")
	beat(t, c)
	key := "one-request-token-12345678"
	a, again, err := c.Reserve(request("room-1"), key)
	if err != nil || again {
		t.Fatalf("unexpected: %v %v", err, again)
	}
	b, again, err := c.Reserve(request("room-1"), key)
	if err != nil || !again || a.ID != b.ID {
		t.Fatalf("idempotency broken: %v", err)
	}
	changed := request("room-1")
	changed.MemoryMiB = 1200
	if _, _, err = c.Reserve(changed, key); err != ErrConflict {
		t.Fatalf("expected key conflict: %v", err)
	}
	if _, _, err = c.Reserve(request("room-1"), "different-token-1234567"); err != ErrConflict {
		t.Fatalf("expected room uniqueness: %v", err)
	}
	ok, err := c.Release(a.ID)
	if err != nil || !ok {
		t.Fatalf("release failed %v", err)
	}
	if _, err = c.Release(a.ID); err != ErrNotFound {
		t.Fatal("duplicate release should be 404")
	}
}
func TestHighContentionNeverOversubscribes(t *testing.T) {
	c, _ := testControl(t, "")
	beat(t, c)
	var wg sync.WaitGroup
	success := make(chan Allocation, 100)
	for i := 0; i < 100; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			r := request(fmt.Sprintf("room-%03d", i))
			a, _, err := c.Reserve(r, fmt.Sprintf("long-unique-key-%03d-123456", i))
			if err == nil {
				success <- a
			} else if err != ErrNoCapacity {
				t.Errorf("unexpected reserve error: %v", err)
			}
		}(i)
	}
	wg.Wait()
	close(success)
	count := 0
	perNode := map[string]int{}
	for a := range success {
		count++
		perNode[a.NodeID]++
	}
	if count != 4 {
		t.Fatalf("expected four rooms capped by maxRooms; got %d", count)
	}
	for _, n := range []string{"node-a", "node-b"} {
		if perNode[n] != 2 {
			t.Fatalf("node %s: %d", n, perNode[n])
		}
	}
}
func TestPressureAndDrainAreFailClosed(t *testing.T) {
	for _, tc := range []struct {
		name     string
		mutate   func(*Telemetry)
		expected string
	}{
		{"disk", func(v *Telemetry) { v.FreeDiskMiB = 100 }, "disk_pressure"},
		{"cpu", func(v *Telemetry) { v.Load1 = 8 }, "cpu_pressure"},
		{"swap", func(v *Telemetry) { v.SwapUsedMiB = 90 }, "swap_pressure"},
		{"memory", func(v *Telemetry) { v.AvailableMemoryMiB = 300 }, "memory_pressure"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			c, _ := testControl(t, "")
			h := goodTelemetry()
			tc.mutate(&h)
			_ = c.Heartbeat("node-a", h)
			states := c.Nodes()
			if states[0].State != tc.expected {
				t.Fatalf("got %s expected %s", states[0].State, tc.expected)
			}
			if _, _, err := c.Reserve(request("room-1"), "unique-sample-token-123456"); err != ErrNoCapacity {
				t.Fatalf("got %v", err)
			}
		})
	}
	inv := testInv()
	inv.Nodes[0].Draining = true
	c, err := New(inv, "", time.Now)
	if err != nil {
		t.Fatal(err)
	}
	_ = c.Heartbeat("node-a", goodTelemetry())
	if c.Nodes()[0].State != "draining" {
		t.Fatal("draining node eligible")
	}
}
func TestPersistAndReloadFailClosed(t *testing.T) {
	dir := t.TempDir()
	state := filepath.Join(dir, "leases.json")
	c, now := testControl(t, state)
	beat(t, c)
	a, _, err := c.Reserve(request("room-1"), "persistent-key-123456789")
	if err != nil {
		t.Fatal(err)
	}
	perm, err := os.Stat(state)
	if err != nil {
		t.Fatal(err)
	}
	if perm.Mode().Perm() != 0600 {
		t.Fatal("secrets must not be world-readable")
	}
	d, err := New(testInv(), state, func() time.Time { return *now })
	if err != nil {
		t.Fatal(err)
	}
	if len(d.Allocations()) != 1 {
		t.Fatal("lost reservation")
	}
	if _, _, err := d.Reserve(request("room-1"), "persistent-key-123456789"); err != nil {
		t.Fatal(err)
	}
	if err = os.WriteFile(state, []byte("this is corrupt"), 0600); err != nil {
		t.Fatal(err)
	}
	if _, err = New(testInv(), state, nil); err == nil {
		t.Fatal("corrupted state must reject restart")
	}
	if _, err = d.Renew(a.ID, 120); err != nil {
		t.Fatal(err)
	}
}
func TestRejectBadInputsAndTelemetry(t *testing.T) {
	c, _ := testControl(t, "")
	beat(t, c)
	for _, x := range []Request{{}, {RoomID: "../etc", Region: "us-test", CPUMilli: 1, MemoryMiB: 1, TTLSeconds: 300}, {RoomID: "ok", Region: "us-test", CPUMilli: -1, MemoryMiB: 1, TTLSeconds: 300}} {
		if _, _, err := c.Reserve(x, "unique-request-id-12345"); err != ErrInvalid {
			t.Fatal(err)
		}
	}
	if _, _, err := c.Reserve(request("room-1"), "short"); err != ErrInvalid {
		t.Fatal(err)
	}
	bad := goodTelemetry()
	bad.SwapUsedMiB = bad.SwapTotalMiB + 1
	if err := c.Heartbeat("node-a", bad); err != ErrInvalid {
		t.Fatal(err)
	}
	if _, err := c.Renew("missing", 20); err != ErrInvalid {
		t.Fatal(err)
	}
}

func TestStaleAndRegionIsolation(t *testing.T) {
	c, now := testControl(t, "")
	beat(t, c)
	*now = now.Add(21 * time.Second)
	for _, n := range c.Nodes() {
		if n.State != "stale" {
			t.Fatalf("expected stale, got %s", n.State)
		}
	}
	if _, _, err := c.Reserve(request("room-stale"), "stale-token-123456789"); err != ErrNoCapacity {
		t.Fatal(err)
	}
	_ = c.Heartbeat("node-a", goodTelemetry())
	r := request("room-bad-region")
	r.Region = "eu-test"
	if _, _, err := c.Reserve(r, "wrong-region-123456789"); err != ErrNoCapacity {
		t.Fatal(err)
	}
}

func TestUnstartedReservationsReduceAdmissionByObservedMemory(t *testing.T) {
	c, _ := testControl(t, "")
	constrained := goodTelemetry()
	constrained.AvailableMemoryMiB = 1800
	_ = c.Heartbeat("node-a", constrained)
	if _, _, err := c.Reserve(request("room-not-enough"), "ram-shortfall-123456789"); err != ErrNoCapacity {
		t.Fatal("must not schedule into live RAM shortfall")
	}
	constrained.AvailableMemoryMiB = 3600
	_ = c.Heartbeat("node-a", constrained)
	if _, _, err := c.Reserve(request("room-1"), "room-first-123456789"); err != nil {
		t.Fatal(err)
	}
	if _, _, err := c.Reserve(request("room-2"), "room-second-123456789"); err != nil {
		t.Fatal(err)
	}
	// 3600 - 1024 safety floor - 2000 pending reservations = 576 MiB; third room fails.
	if _, _, err := c.Reserve(request("room-3"), "room-third-123456789"); err != ErrNoCapacity {
		t.Fatalf("got %v", err)
	}
}
