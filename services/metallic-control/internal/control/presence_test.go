package control

import (
	"errors"
	"testing"
	"time"
)

func reservedRoom(t *testing.T, c *Controller, room, key string) Allocation {
	t.Helper()
	a, _, err := c.Reserve(request(room), key)
	if err != nil {
		t.Fatal(err)
	}
	return a
}

func TestPresenceIsUnmeasuredWithoutReports(t *testing.T) {
	c, _ := testControl(t, "")
	beat(t, c)
	reservedRoom(t, c, "room-1", "key-at-least-123456789")
	p := c.Presence()
	if p.Measured || p.Players != 0 || p.Rooms != 0 || p.ObservedAt != nil {
		t.Fatalf("a reservation must not imply presence: %+v", p)
	}
}

func TestPresenceSumsOnlySuppliedRoomReports(t *testing.T) {
	c, now := testControl(t, "")
	beat(t, c)
	a1 := reservedRoom(t, c, "room-1", "key-at-least-123456789")
	a2 := reservedRoom(t, c, "room-2", "key-at-least-987654321")
	reservedRoom(t, c, "room-3", "key-at-least-555555555") // never reports
	if err := c.ReportPresence(a1.NodeID, "room-1", 3); err != nil {
		t.Fatal(err)
	}
	if err := c.ReportPresence(a2.NodeID, "room-2", 4); err != nil {
		t.Fatal(err)
	}
	// A newer report replaces, never adds to, the previous one.
	if err := c.ReportPresence(a1.NodeID, "room-1", 2); err != nil {
		t.Fatal(err)
	}
	p := c.Presence()
	if !p.Measured || p.Players != 6 || p.Rooms != 2 || p.ObservedAt == nil || !p.ObservedAt.Equal(*now) {
		t.Fatalf("unexpected aggregate: %+v", p)
	}
}

func TestPresenceRejectsUncappedUnknownOrWrongNodeReports(t *testing.T) {
	c, _ := testControl(t, "")
	beat(t, c)
	a := reservedRoom(t, c, "room-1", "key-at-least-123456789")
	other := "node-a"
	if a.NodeID == "node-a" {
		other = "node-b"
	}
	for name, tc := range map[string]struct {
		node, room string
		players    int
		want       error
	}{
		"negative":     {a.NodeID, "room-1", -1, ErrInvalid},
		"over cap":     {a.NodeID, "room-1", MaxPlayersPerRoom + 1, ErrInvalid},
		"bad room id":  {a.NodeID, "bad room", 1, ErrInvalid},
		"unknown room": {a.NodeID, "room-9", 1, ErrNotFound},
		"wrong node":   {other, "room-1", 1, ErrNotFound},
		"unknown node": {"node-z", "room-1", 1, ErrNotFound},
	} {
		if err := c.ReportPresence(tc.node, tc.room, tc.players); !errors.Is(err, tc.want) {
			t.Fatalf("%s: got %v want %v", name, err, tc.want)
		}
	}
	if p := c.Presence(); p.Measured {
		t.Fatalf("rejected reports must not count: %+v", p)
	}
	if err := c.ReportPresence(a.NodeID, "room-1", MaxPlayersPerRoom); err != nil {
		t.Fatalf("cap itself must be accepted: %v", err)
	}
}

func TestPresenceExpiresWhenStaleOrReleased(t *testing.T) {
	c, now := testControl(t, "")
	beat(t, c)
	a := reservedRoom(t, c, "room-1", "key-at-least-123456789")
	if err := c.ReportPresence(a.NodeID, "room-1", 5); err != nil {
		t.Fatal(err)
	}
	*now = now.Add(c.staleAfter + time.Second)
	if p := c.Presence(); p.Measured || p.Players != 0 {
		t.Fatalf("stale report still counted: %+v", p)
	}
	beat(t, c)
	if err := c.ReportPresence(a.NodeID, "room-1", 5); err != nil {
		t.Fatal(err)
	}
	if _, err := c.Release(a.ID); err != nil {
		t.Fatal(err)
	}
	if p := c.Presence(); p.Measured {
		t.Fatalf("released room still counted: %+v", p)
	}
}
