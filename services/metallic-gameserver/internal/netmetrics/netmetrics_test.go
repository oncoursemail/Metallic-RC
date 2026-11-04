package netmetrics

import (
	"testing"
	"time"
)

func TestBytesPerPlayerAndLatePackets(t *testing.T) {
	m := New(2)
	m.RecordInput("a", 1, 100)
	m.RecordInput("a", 2, 100)
	m.RecordInput("a", 4, 100)
	// seq 3 arrives after 4: late but within tolerance
	if got := m.RecordInput("a", 3, 50); got != Late {
		t.Fatalf("want Late, got %v", got)
	}
	// seq 1 is 3 behind newest (4) with tolerance 2: dropped
	if got := m.RecordInput("a", 1, 50); got != Dropped {
		t.Fatalf("want Dropped, got %v", got)
	}
	// duplicate of newest
	if got := m.RecordInput("a", 4, 10); got != Dropped {
		t.Fatalf("want Dropped for duplicate, got %v", got)
	}
	m.RecordInput("b", 1, 40)
	r := m.Report()
	a := r.Players["a"]
	if a.Bytes != 410 || a.Packets != 6 || a.Late != 1 || a.Dropped != 2 {
		t.Fatalf("bad a: %+v", a)
	}
	if r.Players["b"].Bytes != 40 {
		t.Fatalf("bad b: %+v", r.Players["b"])
	}
	if r.BytesPerPlayer != 225 {
		t.Fatalf("bytes per player = %v", r.BytesPerPlayer)
	}
}

func TestRTTAndTickCPU(t *testing.T) {
	m := New(2)
	for _, ms := range []int{10, 20, 30, 40, 100} {
		m.RecordRTT("a", time.Duration(ms)*time.Millisecond)
	}
	m.RecordTick(2 * time.Millisecond)
	m.RecordTick(4 * time.Millisecond)
	r := m.Report()
	if r.Players["a"].RTTMean != 40*time.Millisecond || r.Players["a"].RTTMax != 100*time.Millisecond {
		t.Fatalf("bad rtt: %+v", r.Players["a"])
	}
	if r.Players["a"].RTTP95 != 100*time.Millisecond {
		t.Fatalf("p95 = %v", r.Players["a"].RTTP95)
	}
	if r.Ticks != 2 || r.TickMean != 3*time.Millisecond || r.TickMax != 4*time.Millisecond {
		t.Fatalf("bad tick: %+v", r)
	}
}
