// Package rooms keeps versioned, reviewer-gated room revisions in memory.
package rooms

import (
	"errors"
	"sync"
)

// Status is the moderation state of a revision.
type Status string

const (
	StatusPending  Status = "pending"
	StatusApproved Status = "approved"
	StatusRejected Status = "rejected"
)

var (
	ErrInvalid      = errors.New("invalid revision request")
	ErrNotFound     = errors.New("revision not found")
	ErrNotPending   = errors.New("revision is not pending")
	ErrNotApproved  = errors.New("revision was never approved")
	ErrSelfApproval = errors.New("creator cannot review own revision")
)

// Revision is one published version of a room.
type Revision struct {
	Room     string
	Version  int
	Creator  string
	Content  string // content hash of the validated room bundle
	Status   Status
	Reviewer string
	Reason   string
}

type roomState struct {
	revisions []*Revision
	live      int // version number, 0 when none
}

// Store holds revisions per room. It is safe for concurrent use.
type Store struct {
	mu    sync.Mutex
	rooms map[string]*roomState
}

// NewStore returns an empty store.
func NewStore() *Store {
	return &Store{rooms: map[string]*roomState{}}
}

// Publish records a new pending revision and returns it.
func (s *Store) Publish(room, creator, content string) (Revision, error) {
	if room == "" || creator == "" || content == "" {
		return Revision{}, ErrInvalid
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	st := s.rooms[room]
	if st == nil {
		st = &roomState{}
		s.rooms[room] = st
	}
	r := &Revision{Room: room, Version: len(st.revisions) + 1, Creator: creator, Content: content, Status: StatusPending}
	st.revisions = append(st.revisions, r)
	return *r, nil
}

func (s *Store) find(room string, version int) (*roomState, *Revision, error) {
	st := s.rooms[room]
	if st == nil || version < 1 || version > len(st.revisions) {
		return nil, nil, ErrNotFound
	}
	return st, st.revisions[version-1], nil
}

// Approve marks a pending revision approved and makes it live.
func (s *Store) Approve(room string, version int, reviewer string) (Revision, error) {
	if reviewer == "" {
		return Revision{}, ErrInvalid
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	st, r, err := s.find(room, version)
	if err != nil {
		return Revision{}, err
	}
	if r.Status != StatusPending {
		return Revision{}, ErrNotPending
	}
	if r.Creator == reviewer {
		return Revision{}, ErrSelfApproval
	}
	r.Status, r.Reviewer = StatusApproved, reviewer
	st.live = r.Version
	return *r, nil
}

// Reject marks a pending revision rejected with a reason.
func (s *Store) Reject(room string, version int, reviewer, reason string) (Revision, error) {
	if reviewer == "" || reason == "" {
		return Revision{}, ErrInvalid
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	_, r, err := s.find(room, version)
	if err != nil {
		return Revision{}, err
	}
	if r.Status != StatusPending {
		return Revision{}, ErrNotPending
	}
	r.Status, r.Reviewer, r.Reason = StatusRejected, reviewer, reason
	return *r, nil
}

// Rollback makes a previously approved revision live again.
func (s *Store) Rollback(room string, version int, actor string) (Revision, error) {
	if actor == "" {
		return Revision{}, ErrInvalid
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	st, r, err := s.find(room, version)
	if err != nil {
		return Revision{}, err
	}
	if r.Status != StatusApproved {
		return Revision{}, ErrNotApproved
	}
	st.live = r.Version
	return *r, nil
}

// Live returns the revision currently served for a room.
func (s *Store) Live(room string) (Revision, bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	st := s.rooms[room]
	if st == nil || st.live == 0 {
		return Revision{}, false
	}
	return *st.revisions[st.live-1], true
}
