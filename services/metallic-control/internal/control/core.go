// Package control implements a lab-only, single-leader reservation control plane.
// It does not launch game processes or implement a production session protocol.
package control

import (
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"math"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"time"
)

var (
	ErrInvalid    = errors.New("invalid_input")
	ErrNoCapacity = errors.New("no_capacity")
	ErrConflict   = errors.New("conflict")
	ErrNotFound   = errors.New("not_found")
	ErrTooLarge   = errors.New("payload_too_large")
)

// NodeSpec is operator-owned inventory, not client or agent-supplied capacity.
// Reservation budgets are deliberately smaller than physical device resources.
type NodeSpec struct {
	ID                  string  `json:"id"`
	Region              string  `json:"region"`
	LogicalCPUs         int     `json:"logicalCpus"`
	CPUMilli            int     `json:"cpuMilli"`
	MemoryMiB           int     `json:"memoryMiB"`
	MaxRooms            int     `json:"maxRooms"`
	MinFreeMemoryMiB    int     `json:"minFreeMemoryMiB"`
	MinFreeDiskMiB      int     `json:"minFreeDiskMiB"`
	MaxLoadPerThread    float64 `json:"maxLoadPerThread"`
	MaxSwapUsedFraction float64 `json:"maxSwapUsedFraction"`
	Draining            bool    `json:"draining"`
}

type Inventory struct {
	Nodes []NodeSpec `json:"nodes"`
}

type Telemetry struct {
	AvailableMemoryMiB int     `json:"availableMemoryMiB"`
	FreeDiskMiB        int     `json:"freeDiskMiB"`
	Load1              float64 `json:"load1"`
	SwapUsedMiB        int     `json:"swapUsedMiB"`
	SwapTotalMiB       int     `json:"swapTotalMiB"`
}

type Request struct {
	RoomID     string `json:"roomId"`
	Region     string `json:"region"`
	CPUMilli   int    `json:"cpuMilli"`
	MemoryMiB  int    `json:"memoryMiB"`
	TTLSeconds int    `json:"ttlSeconds"`
}

type Allocation struct {
	InitialTTLSeconds int       `json:"-"`
	ID                string    `json:"id"`
	RoomID            string    `json:"roomId"`
	NodeID            string    `json:"nodeId"`
	Region            string    `json:"region"`
	CPUMilli          int       `json:"cpuMilli"`
	MemoryMiB         int       `json:"memoryMiB"`
	ExpiresAt         time.Time `json:"expiresAt"`
	IdempotencyKey    string    `json:"-"`
}

type persistedAllocation struct {
	Allocation
	InitialTTLSeconds int    `json:"initialTTLSeconds"`
	IdempotencyKey    string `json:"idempotencyKey"`
}

type persistent struct {
	Schema      int                   `json:"schema"`
	Allocations []persistedAllocation `json:"allocations"`
}

type heartbeat struct {
	stats    Telemetry
	observed time.Time
}

type NodeStatus struct {
	NodeID        string     `json:"nodeId"`
	Region        string     `json:"region"`
	State         string     `json:"state"`
	LastSeen      *time.Time `json:"lastSeen,omitempty"`
	CPUMilliFree  int        `json:"cpuMilliFree"`
	MemoryMiBFree int        `json:"memoryMiBFree"`
	ReservedRooms int        `json:"reservedRooms"`
	Telemetry     *Telemetry `json:"telemetry,omitempty"`
}

type Controller struct {
	mu          sync.Mutex
	nodes       map[string]NodeSpec
	heartbeats  map[string]heartbeat
	allocations map[string]Allocation
	path        string
	now         func() time.Time
	staleAfter  time.Duration
}

func New(inv Inventory, path string, now func() time.Time) (*Controller, error) {
	if now == nil {
		now = time.Now
	}
	if len(inv.Nodes) == 0 {
		return nil, fmt.Errorf("%w: at least one configured node required", ErrInvalid)
	}
	c := &Controller{nodes: make(map[string]NodeSpec), heartbeats: make(map[string]heartbeat), allocations: make(map[string]Allocation), path: path, now: now, staleAfter: 20 * time.Second}
	for _, s := range inv.Nodes {
		if !validID(s.ID) || !validID(s.Region) || s.LogicalCPUs <= 0 || s.CPUMilli <= 0 || s.MemoryMiB <= 0 || s.MaxRooms <= 0 || s.MinFreeMemoryMiB < 0 || s.MinFreeDiskMiB < 0 || s.MaxLoadPerThread <= 0 || s.MaxLoadPerThread > 2 || s.MaxSwapUsedFraction <= 0 || s.MaxSwapUsedFraction > 1 || math.IsNaN(s.MaxLoadPerThread) || math.IsInf(s.MaxLoadPerThread, 0) || math.IsNaN(s.MaxSwapUsedFraction) || math.IsInf(s.MaxSwapUsedFraction, 0) {
			return nil, fmt.Errorf("%w: invalid node spec", ErrInvalid)
		}
		if _, exists := c.nodes[s.ID]; exists {
			return nil, fmt.Errorf("%w: duplicate node", ErrInvalid)
		}
		c.nodes[s.ID] = s
	}
	if path != "" {
		b, err := os.ReadFile(path)
		if err != nil && !os.IsNotExist(err) {
			return nil, err
		}
		if err == nil {
			var state persistent
			if json.Unmarshal(b, &state) != nil || state.Schema != 1 {
				return nil, errors.New("invalid persisted state: refuse to reset")
			}
			roomsSeen := map[string]bool{}
			keysSeen := map[string]bool{}
			for _, pa := range state.Allocations {
				a := pa.Allocation
				a.IdempotencyKey = pa.IdempotencyKey
				a.InitialTTLSeconds = pa.InitialTTLSeconds
				if a.InitialTTLSeconds < 30 || a.InitialTTLSeconds > 3600 || !validID(a.ID) || !validID(a.RoomID) || !validID(a.NodeID) || a.CPUMilli <= 0 || a.MemoryMiB <= 0 || a.ExpiresAt.IsZero() || !validKey(a.IdempotencyKey) {
					return nil, errors.New("invalid persisted allocation: refuse to reset")
				}
				if spec, ok := c.nodes[a.NodeID]; !ok || spec.Region != a.Region || a.CPUMilli > spec.CPUMilli || a.MemoryMiB > spec.MemoryMiB || roomsSeen[a.RoomID] || keysSeen[a.IdempotencyKey] {
					return nil, errors.New("persisted allocation violates configured inventory or uniqueness: refuse to reset")
				}
				roomsSeen[a.RoomID] = true
				keysSeen[a.IdempotencyKey] = true
				if _, ok := c.allocations[a.ID]; ok {
					return nil, errors.New("duplicate persisted allocation")
				}
				c.allocations[a.ID] = a
			}
		}
	}
	return c, nil
}

func validID(s string) bool {
	if len(s) < 1 || len(s) > 96 {
		return false
	}
	for _, r := range s {
		if r >= 'a' && r <= 'z' || r >= 'A' && r <= 'Z' || r >= '0' && r <= '9' || r == '-' || r == '_' {
			continue
		}
		return false
	}
	return true
}
func validKey(s string) bool { return len(s) >= 12 && len(s) <= 128 && validKeyChars(s) }
func validKeyChars(s string) bool {
	for _, r := range s {
		if r <= ' ' || r > '~' {
			return false
		}
	}
	return true
}
func (r Request) valid() bool {
	return validID(r.RoomID) && validID(r.Region) && r.CPUMilli > 0 && r.CPUMilli <= 100000 && r.MemoryMiB > 0 && r.MemoryMiB <= 1048576 && r.TTLSeconds >= 30 && r.TTLSeconds <= 3600
}

// Heartbeat takes an already authenticated node ID; collectedAt is server-controlled.
func (c *Controller) Heartbeat(id string, h Telemetry) error {
	c.mu.Lock()
	defer c.mu.Unlock()
	if _, ok := c.nodes[id]; !ok {
		return ErrNotFound
	}
	if h.AvailableMemoryMiB < 0 || h.FreeDiskMiB < 0 || h.Load1 < 0 || math.IsNaN(h.Load1) || math.IsInf(h.Load1, 0) || h.SwapUsedMiB < 0 || h.SwapTotalMiB < 0 || h.SwapUsedMiB > h.SwapTotalMiB {
		return ErrInvalid
	}
	c.heartbeats[id] = heartbeat{stats: h, observed: c.now().UTC()}
	return nil
}

func (c *Controller) active(now time.Time) map[string]Allocation {
	m := make(map[string]Allocation, len(c.allocations))
	for id, a := range c.allocations {
		if a.ExpiresAt.After(now) {
			m[id] = a
		}
	}
	return m
}

func (c *Controller) status(spec NodeSpec, now time.Time, allocations map[string]Allocation) NodeStatus {
	out := NodeStatus{NodeID: spec.ID, Region: spec.Region, State: "offline", CPUMilliFree: spec.CPUMilli, MemoryMiBFree: spec.MemoryMiB}
	for _, a := range allocations {
		if a.NodeID == spec.ID {
			out.CPUMilliFree -= a.CPUMilli
			out.MemoryMiBFree -= a.MemoryMiB
			out.ReservedRooms++
		}
	}
	if spec.Draining {
		out.State = "draining"
		return out
	}
	h, exists := c.heartbeats[spec.ID]
	if !exists {
		return out
	}
	t := h.observed
	out.LastSeen = &t
	data := h.stats
	out.Telemetry = &data
	if now.Sub(h.observed) > c.staleAfter {
		out.State = "stale"
		return out
	}
	if h.stats.FreeDiskMiB < spec.MinFreeDiskMiB {
		out.State = "disk_pressure"
		return out
	}
	if h.stats.SwapTotalMiB > 0 && float64(h.stats.SwapUsedMiB)/float64(h.stats.SwapTotalMiB) > spec.MaxSwapUsedFraction {
		out.State = "swap_pressure"
		return out
	}
	if h.stats.Load1 > float64(spec.LogicalCPUs)*spec.MaxLoadPerThread {
		out.State = "cpu_pressure"
		return out
	}
	if h.stats.AvailableMemoryMiB < spec.MinFreeMemoryMiB {
		out.State = "memory_pressure"
		return out
	}
	if out.CPUMilliFree <= 0 || out.MemoryMiBFree <= 0 || out.ReservedRooms >= spec.MaxRooms {
		out.State = "at_capacity"
		return out
	}
	out.State = "ready"
	return out
}

func (c *Controller) Nodes() []NodeStatus {
	c.mu.Lock()
	defer c.mu.Unlock()
	now := c.now().UTC()
	active := c.active(now)
	statuses := make([]NodeStatus, 0, len(c.nodes))
	for _, spec := range c.nodes {
		statuses = append(statuses, c.status(spec, now, active))
	}
	sort.Slice(statuses, func(i, j int) bool { return statuses[i].NodeID < statuses[j].NodeID })
	return statuses
}

func (c *Controller) Allocations() []Allocation {
	c.mu.Lock()
	defer c.mu.Unlock()
	active := c.active(c.now().UTC())
	out := make([]Allocation, 0, len(active))
	for _, a := range active {
		a.IdempotencyKey = ""
		out = append(out, a)
	}
	sort.Slice(out, func(i, j int) bool { return out[i].ID < out[j].ID })
	return out
}

func (c *Controller) persist(next map[string]Allocation) error {
	if c.path == "" {
		return nil
	} // isolated in-memory test mode only
	arr := make([]persistedAllocation, 0, len(next))
	for _, a := range next {
		arr = append(arr, persistedAllocation{Allocation: a, IdempotencyKey: a.IdempotencyKey, InitialTTLSeconds: a.InitialTTLSeconds})
	}
	sort.Slice(arr, func(i, j int) bool { return arr[i].ID < arr[j].ID })
	data, err := json.Marshal(persistent{Schema: 1, Allocations: arr})
	if err != nil {
		return err
	}
	dir := filepath.Dir(c.path)
	// Data directory must already exist with operator-managed restrictive permissions.
	f, err := os.CreateTemp(dir, ".metallic-state-*")
	if err != nil {
		return err
	}
	tmp := f.Name()
	defer os.Remove(tmp)
	if err = f.Chmod(0600); err != nil {
		f.Close()
		return err
	}
	if _, err = f.Write(data); err != nil {
		f.Close()
		return err
	}
	if err = f.Sync(); err != nil {
		f.Close()
		return err
	}
	if err = f.Close(); err != nil {
		return err
	}
	if err = os.Rename(tmp, c.path); err != nil {
		return err
	}
	d, err := os.Open(dir)
	if err != nil {
		return err
	}
	defer d.Close()
	return d.Sync()
}

func newID() (string, error) {
	b := make([]byte, 16)
	if _, err := rand.Read(b); err != nil {
		return "", err
	}
	return hex.EncodeToString(b), nil
}

func requestEquals(r Request, a Allocation) bool {
	return r.RoomID == a.RoomID && r.Region == a.Region && r.CPUMilli == a.CPUMilli && r.MemoryMiB == a.MemoryMiB && r.TTLSeconds == a.InitialTTLSeconds
}

func (c *Controller) Reserve(r Request, key string) (Allocation, bool, error) {
	if !r.valid() || !validKey(key) {
		return Allocation{}, false, ErrInvalid
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	now := c.now().UTC()
	live := c.active(now)
	for _, a := range live {
		if a.IdempotencyKey == key {
			if !requestEquals(r, a) {
				return Allocation{}, false, ErrConflict
			}
			a.IdempotencyKey = ""
			return a, true, nil
		}
		if a.RoomID == r.RoomID {
			return Allocation{}, false, ErrConflict
		}
	}
	var candidates []NodeStatus
	for _, spec := range c.nodes {
		if spec.Region != r.Region {
			continue
		}
		s := c.status(spec, now, live)
		if s.State != "ready" || s.ReservedRooms >= spec.MaxRooms || s.CPUMilliFree < r.CPUMilli || s.MemoryMiBFree < r.MemoryMiB {
			continue
		}
		// Conservative: live OS MemAvailable minus reserve and still-unstarted reservations.
		if s.Telemetry.AvailableMemoryMiB-spec.MinFreeMemoryMiB-(spec.MemoryMiB-s.MemoryMiBFree) < r.MemoryMiB {
			continue
		}
		candidates = append(candidates, s)
	}
	if len(candidates) == 0 {
		return Allocation{}, false, ErrNoCapacity
	}
	// Least-reserved room first; stable tie-break ensures deterministic scheduling.
	sort.Slice(candidates, func(i, j int) bool {
		if candidates[i].ReservedRooms == candidates[j].ReservedRooms {
			return candidates[i].NodeID < candidates[j].NodeID
		}
		return candidates[i].ReservedRooms < candidates[j].ReservedRooms
	})
	id, err := newID()
	if err != nil {
		return Allocation{}, false, err
	}
	a := Allocation{InitialTTLSeconds: r.TTLSeconds, ID: id, RoomID: r.RoomID, NodeID: candidates[0].NodeID, Region: r.Region, CPUMilli: r.CPUMilli, MemoryMiB: r.MemoryMiB, ExpiresAt: now.Add(time.Duration(r.TTLSeconds) * time.Second), IdempotencyKey: key}
	live[id] = a
	if err = c.persist(live); err != nil {
		return Allocation{}, false, err
	}
	c.allocations = live
	a.IdempotencyKey = ""
	return a, false, nil
}

func (c *Controller) Release(id string) (bool, error) {
	if !validID(id) {
		return false, ErrInvalid
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	live := c.active(c.now().UTC())
	_, exists := live[id]
	if !exists {
		return false, ErrNotFound
	}
	delete(live, id)
	if err := c.persist(live); err != nil {
		return false, err
	}
	c.allocations = live
	return true, nil
}

func (c *Controller) Renew(id string, seconds int) (Allocation, error) {
	if !validID(id) || seconds < 30 || seconds > 3600 {
		return Allocation{}, ErrInvalid
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	now := c.now().UTC()
	live := c.active(now)
	a, ok := live[id]
	if !ok {
		return Allocation{}, ErrNotFound
	}
	// Never turn an expired reservation back into an active one.
	a.ExpiresAt = now.Add(time.Duration(seconds) * time.Second)
	live[id] = a
	if err := c.persist(live); err != nil {
		return Allocation{}, err
	}
	c.allocations = live
	a.IdempotencyKey = ""
	return a, nil
}

func LoadInventory(path string) (Inventory, error) {
	b, err := os.ReadFile(path)
	if err != nil {
		return Inventory{}, err
	}
	var inv Inventory
	dec := json.NewDecoder(strings.NewReader(string(b)))
	dec.DisallowUnknownFields()
	if err = dec.Decode(&inv); err != nil {
		return Inventory{}, err
	}
	if err = dec.Decode(new(interface{})); err == io.EOF {
		return inv, nil
	}
	return Inventory{}, ErrInvalid
}
