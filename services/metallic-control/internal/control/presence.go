package control

import "time"

// MaxPlayersPerRoom caps a single supplied room presence count. Larger values
// are rejected, never clamped, so an implausible report cannot be published.
const MaxPlayersPerRoom = 1000

type presenceReport struct {
	nodeID   string
	players  int
	observed time.Time
}

// PresenceAggregate sums only fresh, node-supplied room counts. Measured is
// false when there are none; zero Players then means "unknown", not "empty".
type PresenceAggregate struct {
	Measured   bool       `json:"measured"`
	Players    int        `json:"players"`
	Rooms      int        `json:"rooms"`
	ObservedAt *time.Time `json:"observedAt,omitempty"`
}

// ReportPresence records an already authenticated node's own player count for
// a room it holds an active reservation for. A new report replaces the old one.
func (c *Controller) ReportPresence(nodeID, roomID string, players int) error {
	if !validID(nodeID) || !validID(roomID) || players < 0 || players > MaxPlayersPerRoom {
		return ErrInvalid
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	now := c.now().UTC()
	for _, a := range c.active(now) {
		if a.RoomID == roomID && a.NodeID == nodeID {
			if c.presence == nil {
				c.presence = make(map[string]presenceReport)
			}
			c.presence[roomID] = presenceReport{nodeID: nodeID, players: players, observed: now}
			return nil
		}
	}
	return ErrNotFound
}

// Presence aggregates reports that are fresh and still match an active
// reservation. Reservations without a report contribute nothing.
func (c *Controller) Presence() PresenceAggregate {
	c.mu.Lock()
	defer c.mu.Unlock()
	now := c.now().UTC()
	var out PresenceAggregate
	for _, a := range c.active(now) {
		r, ok := c.presence[a.RoomID]
		if !ok || r.nodeID != a.NodeID || now.Sub(r.observed) > c.staleAfter {
			continue
		}
		out.Measured = true
		out.Players += r.players
		out.Rooms++
		if out.ObservedAt == nil || r.observed.After(*out.ObservedAt) {
			t := r.observed
			out.ObservedAt = &t
		}
	}
	return out
}
