package rooms

import (
	"errors"
	"testing"
)

func TestPublishApproveRollback(t *testing.T) {
	s := NewStore()
	r1, err := s.Publish("room-a", "creator-1", "hash-1")
	if err != nil || r1.Version != 1 || r1.Status != StatusPending {
		t.Fatalf("publish v1: %+v %v", r1, err)
	}
	if _, ok := s.Live("room-a"); ok {
		t.Fatal("pending revision must not be live")
	}
	if _, err := s.Approve("room-a", 1, "creator-1"); !errors.Is(err, ErrSelfApproval) {
		t.Fatalf("self approval: %v", err)
	}
	if _, err := s.Approve("room-a", 1, "reviewer-1"); err != nil {
		t.Fatal(err)
	}
	if live, _ := s.Live("room-a"); live.Version != 1 {
		t.Fatalf("live = %+v", live)
	}
	s.Publish("room-a", "creator-1", "hash-2")
	s.Approve("room-a", 2, "reviewer-1")
	if live, _ := s.Live("room-a"); live.Version != 2 {
		t.Fatalf("live = %+v", live)
	}
	live, err := s.Rollback("room-a", 1, "reviewer-1")
	if err != nil || live.Version != 1 {
		t.Fatalf("rollback: %+v %v", live, err)
	}
	if got, _ := s.Live("room-a"); got.Version != 1 {
		t.Fatalf("live after rollback = %+v", got)
	}
}

func TestRollbackRequiresApprovedRevision(t *testing.T) {
	s := NewStore()
	s.Publish("room-a", "c", "h1")
	if _, err := s.Rollback("room-a", 1, "r"); !errors.Is(err, ErrNotApproved) {
		t.Fatalf("got %v", err)
	}
	if _, err := s.Rollback("room-a", 9, "r"); !errors.Is(err, ErrNotFound) {
		t.Fatalf("got %v", err)
	}
	if _, err := s.Publish("", "c", "h"); !errors.Is(err, ErrInvalid) {
		t.Fatalf("got %v", err)
	}
}

func TestRejectAndDoubleDecision(t *testing.T) {
	s := NewStore()
	s.Publish("room-a", "c", "h1")
	if _, err := s.Reject("room-a", 1, "r", "bad mesh"); err != nil {
		t.Fatal(err)
	}
	if _, err := s.Approve("room-a", 1, "r"); !errors.Is(err, ErrNotPending) {
		t.Fatalf("got %v", err)
	}
	if _, ok := s.Live("room-a"); ok {
		t.Fatal("rejected must not be live")
	}
}
