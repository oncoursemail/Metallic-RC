// Package netmetrics measures per-player RTT, bytes and late-packet behavior
// and per-tick server CPU time for the authoritative game server.
package netmetrics

import (
	"math"
	"sort"
	"sync"
	"time"
)

// Disposition says how an input packet was classified on arrival.
type Disposition int

const (
	// OnTime packets carry a sequence newer than any seen before.
	OnTime Disposition = iota
	// Late packets arrive out of order but within the tolerance window.
	Late
	// Dropped packets are duplicates or older than the tolerance window.
	Dropped
)

type player struct {
	bytes, packets, late, dropped int
	newest                        uint32
	any                           bool
	seen                          map[uint32]bool
	rtts                          []time.Duration
}

// Metrics collects measurements; it is safe for concurrent use.
type Metrics struct {
	mu        sync.Mutex
	tolerance uint32
	players   map[string]*player
	ticks     []time.Duration
}

// New returns Metrics that accepts out-of-order inputs up to tolerance
// sequence numbers behind the newest one.
func New(tolerance uint32) *Metrics {
	return &Metrics{tolerance: tolerance, players: map[string]*player{}}
}

func (m *Metrics) get(id string) *player {
	p := m.players[id]
	if p == nil {
		p = &player{seen: map[uint32]bool{}}
		m.players[id] = p
	}
	return p
}

// RecordInput counts a received packet of size bytes and classifies it.
func (m *Metrics) RecordInput(id string, seq uint32, size int) Disposition {
	m.mu.Lock()
	defer m.mu.Unlock()
	p := m.get(id)
	p.bytes += size
	p.packets++
	switch {
	case !p.any || seq > p.newest:
		p.any, p.newest = true, seq
		p.seen[seq] = true
		for s := range p.seen {
			if p.newest-s > m.tolerance {
				delete(p.seen, s)
			}
		}
		return OnTime
	case p.newest-seq > m.tolerance || p.seen[seq]:
		p.dropped++
		return Dropped
	}
	p.seen[seq] = true
	p.late++
	return Late
}

// RecordRTT adds a round-trip-time sample for a player.
func (m *Metrics) RecordRTT(id string, rtt time.Duration) {
	m.mu.Lock()
	defer m.mu.Unlock()
	p := m.get(id)
	p.rtts = append(p.rtts, rtt)
}

// RecordTick adds the CPU time one simulation tick took.
func (m *Metrics) RecordTick(d time.Duration) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.ticks = append(m.ticks, d)
}

// PlayerReport summarizes one player.
type PlayerReport struct {
	Bytes, Packets, Late, Dropped int
	RTTMean, RTTP95, RTTMax       time.Duration
}

// Report summarizes all measurements.
type Report struct {
	Players        map[string]PlayerReport
	BytesPerPlayer float64
	Ticks          int
	TickMean       time.Duration
	TickMax        time.Duration
}

// Report returns a snapshot of the current measurements.
func (m *Metrics) Report() Report {
	m.mu.Lock()
	defer m.mu.Unlock()
	r := Report{Players: map[string]PlayerReport{}, Ticks: len(m.ticks)}
	total := 0
	for id, p := range m.players {
		pr := PlayerReport{Bytes: p.bytes, Packets: p.packets, Late: p.late, Dropped: p.dropped}
		pr.RTTMean, pr.RTTP95, pr.RTTMax = summarize(p.rtts)
		r.Players[id] = pr
		total += p.bytes
	}
	if n := len(m.players); n > 0 {
		r.BytesPerPlayer = float64(total) / float64(n)
	}
	r.TickMean, _, r.TickMax = summarize(m.ticks)
	return r
}

func summarize(ds []time.Duration) (mean, p95, max time.Duration) {
	if len(ds) == 0 {
		return
	}
	s := append([]time.Duration(nil), ds...)
	sort.Slice(s, func(i, j int) bool { return s[i] < s[j] })
	var sum time.Duration
	for _, d := range s {
		sum += d
	}
	rank := int(math.Ceil(0.95 * float64(len(s))))
	return sum / time.Duration(len(s)), s[rank-1], s[len(s)-1]
}
