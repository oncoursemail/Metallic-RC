// Package maker implements an in-memory, lab-only Maker Pen world model:
// authorized creators place, edit and delete owned objects, and each creator
// can undo their own operations without clobbering other creators' changes.
// It has no network protocol or persistence.
package maker

import (
	"errors"
	"math"
	"sync"
)

var (
	ErrInvalid       = errors.New("invalid_input")
	ErrForbidden     = errors.New("forbidden")
	ErrConflict      = errors.New("conflict")
	ErrNotFound      = errors.New("not_found")
	ErrNothingToUndo = errors.New("nothing_to_undo")
)

// Transform is position, rotation (Euler degrees) and scale of an object.
type Transform struct {
	Position [3]float64 `json:"position"`
	Rotation [3]float64 `json:"rotation"`
	Scale    [3]float64 `json:"scale"`
}

// Object is a placed world object. Version changes on every write.
type Object struct {
	ID        string    `json:"id"`
	Kind      string    `json:"kind"`
	Owner     string    `json:"owner"`
	Transform Transform `json:"transform"`
	Version   uint64    `json:"version"`
}

// undoEntry restores before (nil = absent) provided the object is still
// exactly in the state this creator's operation produced (after, nil = absent).
type undoEntry struct {
	id     string
	before *Object
	after  *Object
}

type World struct {
	mu       sync.Mutex
	creators map[string]bool
	kinds    map[string]bool
	objects  map[string]Object
	undo     map[string][]undoEntry
	version  uint64
}

func NewWorld(creators, kinds []string) *World {
	w := &World{
		creators: map[string]bool{},
		kinds:    map[string]bool{},
		objects:  map[string]Object{},
		undo:     map[string][]undoEntry{},
	}
	for _, c := range creators {
		w.creators[c] = true
	}
	for _, k := range kinds {
		w.kinds[k] = true
	}
	return w
}

func validTransform(t Transform) bool {
	for _, v := range [][3]float64{t.Position, t.Rotation, t.Scale} {
		for _, f := range v {
			if math.IsNaN(f) || math.IsInf(f, 0) {
				return false
			}
		}
	}
	for _, f := range t.Scale {
		if f <= 0 {
			return false
		}
	}
	return true
}

func (w *World) Get(id string) (Object, bool) {
	w.mu.Lock()
	defer w.mu.Unlock()
	o, ok := w.objects[id]
	return o, ok
}

// write stores o with a fresh version and returns the stored copy.
func (w *World) write(o Object) Object {
	w.version++
	o.Version = w.version
	w.objects[o.ID] = o
	return o
}

func (w *World) push(creator string, e undoEntry) {
	w.undo[creator] = append(w.undo[creator], e)
}

func ptr(o Object) *Object { return &o }

func (w *World) Place(creator, id, kind string, t Transform) error {
	w.mu.Lock()
	defer w.mu.Unlock()
	if !w.creators[creator] {
		return ErrForbidden
	}
	if id == "" || !w.kinds[kind] || !validTransform(t) {
		return ErrInvalid
	}
	if _, ok := w.objects[id]; ok {
		return ErrConflict
	}
	o := w.write(Object{ID: id, Kind: kind, Owner: creator, Transform: t})
	w.push(creator, undoEntry{id: id, after: ptr(o)})
	return nil
}

// owned returns the object if creator is authorized and owns it.
func (w *World) owned(creator, id string) (Object, error) {
	if !w.creators[creator] {
		return Object{}, ErrForbidden
	}
	o, ok := w.objects[id]
	if !ok {
		return Object{}, ErrNotFound
	}
	if o.Owner != creator {
		return Object{}, ErrForbidden
	}
	return o, nil
}

func (w *World) Edit(creator, id string, t Transform) error {
	w.mu.Lock()
	defer w.mu.Unlock()
	old, err := w.owned(creator, id)
	if err != nil {
		return err
	}
	if !validTransform(t) {
		return ErrInvalid
	}
	before := old
	old.Transform = t
	n := w.write(old)
	w.push(creator, undoEntry{id: id, before: ptr(before), after: ptr(n)})
	return nil
}

func (w *World) Delete(creator, id string) error {
	w.mu.Lock()
	defer w.mu.Unlock()
	old, err := w.owned(creator, id)
	if err != nil {
		return err
	}
	delete(w.objects, id)
	w.push(creator, undoEntry{id: id, before: ptr(old)})
	return nil
}

// Undo reverts the creator's most recent operation. If another creator has
// changed the object since, it returns ErrConflict and keeps the entry.
func (w *World) Undo(creator string) error {
	w.mu.Lock()
	defer w.mu.Unlock()
	if !w.creators[creator] {
		return ErrForbidden
	}
	stack := w.undo[creator]
	if len(stack) == 0 {
		return ErrNothingToUndo
	}
	e := stack[len(stack)-1]
	cur, present := w.objects[e.id]
	if present != (e.after != nil) || (present && cur.Version != e.after.Version) {
		return ErrConflict
	}
	if e.before == nil {
		delete(w.objects, e.id)
	} else {
		// Keep the original version so this creator's earlier entries still match.
		w.objects[e.id] = *e.before
	}
	w.undo[creator] = stack[:len(stack)-1]
	return nil
}
