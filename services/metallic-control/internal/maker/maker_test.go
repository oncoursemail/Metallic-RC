package maker

import (
	"errors"
	"testing"
)

func newWorld() *World {
	return NewWorld([]string{"alice", "bob"}, []string{"cube", "ramp"})
}

func unit() Transform {
	return Transform{Scale: [3]float64{1, 1, 1}}
}

func TestPlaceEditDeleteAndUndoByTwoCreators(t *testing.T) {
	w := newWorld()
	if err := w.Place("alice", "a1", "cube", unit()); err != nil {
		t.Fatal(err)
	}
	if err := w.Place("bob", "b1", "ramp", unit()); err != nil {
		t.Fatal(err)
	}
	moved := unit()
	moved.Position = [3]float64{1, 2, 3}
	if err := w.Edit("alice", "a1", moved); err != nil {
		t.Fatal(err)
	}
	if got, _ := w.Get("a1"); got.Transform.Position != moved.Position {
		t.Fatalf("edit not applied: %+v", got)
	}
	if err := w.Undo("alice"); err != nil {
		t.Fatal(err)
	}
	if got, _ := w.Get("a1"); got.Transform.Position != ([3]float64{}) {
		t.Fatalf("undo edit failed: %+v", got)
	}
	if err := w.Delete("bob", "b1"); err != nil {
		t.Fatal(err)
	}
	if err := w.Undo("bob"); err != nil {
		t.Fatal(err)
	}
	if _, ok := w.Get("b1"); !ok {
		t.Fatal("undo delete should restore object")
	}
	if err := w.Undo("alice"); err != nil { // undo place a1
		t.Fatal(err)
	}
	if _, ok := w.Get("a1"); ok {
		t.Fatal("undo place should remove object")
	}
}

func TestRejectsUnauthorizedIncompatibleAndForeignEdits(t *testing.T) {
	w := newWorld()
	if err := w.Place("mallory", "m", "cube", unit()); !errors.Is(err, ErrForbidden) {
		t.Fatalf("want forbidden, got %v", err)
	}
	if err := w.Place("alice", "x", "torpedo", unit()); !errors.Is(err, ErrInvalid) {
		t.Fatalf("want invalid kind, got %v", err)
	}
	bad := unit()
	bad.Scale = [3]float64{0, 1, 1}
	if err := w.Place("alice", "x", "cube", bad); !errors.Is(err, ErrInvalid) {
		t.Fatalf("want invalid scale, got %v", err)
	}
	_ = w.Place("alice", "a1", "cube", unit())
	if err := w.Place("bob", "a1", "cube", unit()); !errors.Is(err, ErrConflict) {
		t.Fatalf("want conflict, got %v", err)
	}
	if err := w.Edit("bob", "a1", unit()); !errors.Is(err, ErrForbidden) {
		t.Fatalf("want forbidden edit, got %v", err)
	}
	if err := w.Delete("bob", "a1"); !errors.Is(err, ErrForbidden) {
		t.Fatalf("want forbidden delete, got %v", err)
	}
	if err := w.Undo("bob"); !errors.Is(err, ErrNothingToUndo) {
		t.Fatalf("want nothing to undo, got %v", err)
	}
}

func TestUndoNeverOverwritesAnotherCreatorsChange(t *testing.T) {
	w := newWorld()
	_ = w.Place("alice", "a1", "cube", unit())
	moved := unit()
	moved.Position = [3]float64{5, 0, 0}
	_ = w.Edit("alice", "a1", moved)
	_ = w.Delete("alice", "a1")
	// Bob re-uses the freed id; alice's undo of the delete must not clobber it.
	if err := w.Place("bob", "a1", "ramp", unit()); err != nil {
		t.Fatal(err)
	}
	if err := w.Undo("alice"); !errors.Is(err, ErrConflict) {
		t.Fatalf("want conflict, got %v", err)
	}
	if got, _ := w.Get("a1"); got.Owner != "bob" || got.Kind != "ramp" {
		t.Fatalf("bob's object was clobbered: %+v", got)
	}
	// The failed undo stays on the stack, so a later retry is possible.
	_ = w.Delete("bob", "a1")
	if err := w.Undo("alice"); err != nil {
		t.Fatalf("retry after conflict: %v", err)
	}
	if got, _ := w.Get("a1"); got.Owner != "alice" || got.Transform.Position != moved.Position {
		t.Fatalf("restore wrong: %+v", got)
	}
}
