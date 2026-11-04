// Package scene holds shared scene state for concurrent editors. Edits carry
// the revision they were based on; stale edits are rejected with the current
// object so no editor silently overwrites another's work.
package scene

import (
	"errors"
	"fmt"
	"sync"
)

// Object is a scene object at a given revision (0 means it does not exist).
type Object struct {
	ID       string
	Revision uint64
	Props    map[string]string
}

// Edit sets properties on an object, based on the revision the editor saw.
type Edit struct {
	Editor       string
	ObjectID     string
	BaseRevision uint64
	Props        map[string]string
}

// ConflictError reports a stale edit and the object's current state.
type ConflictError struct {
	Current Object
}

func (e *ConflictError) Error() string {
	return fmt.Sprintf("edit conflict on %q: current revision %d", e.Current.ID, e.Current.Revision)
}

// Store is a concurrency-safe set of scene objects.
type Store struct {
	mu      sync.Mutex
	objects map[string]Object
}

// New returns an empty store.
func New() *Store {
	return &Store{objects: map[string]Object{}}
}

func clone(o Object) Object {
	p := make(map[string]string, len(o.Props))
	for k, v := range o.Props {
		p[k] = v
	}
	o.Props = p
	return o
}

// Apply merges the edit's properties into the object if BaseRevision matches
// the current revision, and returns the updated object.
func (s *Store) Apply(e Edit) (Object, error) {
	if e.ObjectID == "" {
		return Object{}, errors.New("object id is required")
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	cur, ok := s.objects[e.ObjectID]
	if !ok {
		cur = Object{ID: e.ObjectID, Props: map[string]string{}}
	}
	if cur.Revision != e.BaseRevision {
		return Object{}, &ConflictError{Current: clone(cur)}
	}
	for k, v := range e.Props {
		cur.Props[k] = v
	}
	cur.Revision++
	s.objects[e.ObjectID] = cur
	return clone(cur), nil
}

// Get returns a copy of the object.
func (s *Store) Get(id string) (Object, bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	o, ok := s.objects[id]
	if !ok {
		return Object{}, false
	}
	return clone(o), true
}
