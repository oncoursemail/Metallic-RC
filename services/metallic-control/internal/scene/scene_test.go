package scene

import (
	"errors"
	"testing"
)

func TestStaleEditIsRejectedNotOverwritten(t *testing.T) {
	s := New()
	obj, err := s.Apply(Edit{Editor: "a", ObjectID: "cube", BaseRevision: 0, Props: map[string]string{"color": "red"}})
	if err != nil || obj.Revision != 1 {
		t.Fatalf("create: %+v %v", obj, err)
	}
	if _, err := s.Apply(Edit{Editor: "a", ObjectID: "cube", BaseRevision: 1, Props: map[string]string{"x": "1"}}); err != nil {
		t.Fatal(err)
	}
	// editor b still works from revision 1
	_, err = s.Apply(Edit{Editor: "b", ObjectID: "cube", BaseRevision: 1, Props: map[string]string{"color": "blue"}})
	var ce *ConflictError
	if !errors.As(err, &ce) || ce.Current.Revision != 2 {
		t.Fatalf("want conflict at revision 2, got %v", err)
	}
	got, _ := s.Get("cube")
	if got.Props["color"] != "red" || got.Props["x"] != "1" {
		t.Fatalf("data lost: %+v", got.Props)
	}
}

func TestCreateOnExistingObjectConflicts(t *testing.T) {
	s := New()
	if _, err := s.Apply(Edit{Editor: "a", ObjectID: "o", Props: map[string]string{"k": "v"}}); err != nil {
		t.Fatal(err)
	}
	if _, err := s.Apply(Edit{Editor: "b", ObjectID: "o", Props: map[string]string{"k": "w"}}); err == nil {
		t.Fatal("expected conflict")
	}
}

func TestGetReturnsCopy(t *testing.T) {
	s := New()
	if _, err := s.Apply(Edit{Editor: "a", ObjectID: "o", Props: map[string]string{"k": "v"}}); err != nil {
		t.Fatal(err)
	}
	g, _ := s.Get("o")
	g.Props["k"] = "mutated"
	g2, _ := s.Get("o")
	if g2.Props["k"] != "v" {
		t.Fatal("aliasing")
	}
}

func TestEmptyObjectIDRejected(t *testing.T) {
	if _, err := New().Apply(Edit{Editor: "a"}); err == nil {
		t.Fatal("expected error")
	}
}
