package catalog

import (
	"errors"
	"testing"
)

func sample() *Index {
	ix := New()
	for _, l := range []Listing{
		{ID: "r1", Name: "Neon Arcade", Tags: []string{"arcade"}, Approved: true, Rating: "everyone"},
		{ID: "r2", Name: "Neon Club", Tags: []string{"social"}, Approved: true, Rating: "teen"},
		{ID: "r3", Name: "Neon Unapproved", Approved: false, Rating: "everyone"},
		{ID: "r4", Name: "Neon Blocked", Approved: true, Blocked: true, Rating: "everyone"},
		{ID: "r5", Name: "Neon Mature", Approved: true, Rating: "mature"},
		{ID: "r6", Name: "Quiet Garden", Tags: []string{"neon"}, Approved: true, Rating: "everyone"},
	} {
		if err := ix.Put(l); err != nil {
			panic(err)
		}
	}
	return ix
}

func ids(r Page) []string {
	var out []string
	for _, l := range r.Items {
		out = append(out, l.ID)
	}
	return out
}

func TestSearchFiltersExcludedAndBlocked(t *testing.T) {
	r, err := sample().Search(Query{Text: "neon", MaxRating: "teen", Limit: 10})
	if err != nil {
		t.Fatal(err)
	}
	got := ids(r)
	want := []string{"r1", "r2", "r6"}
	if len(got) != len(want) {
		t.Fatalf("got %v want %v", got, want)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("got %v want %v", got, want)
		}
	}
	if r.Total != 3 {
		t.Fatalf("total %d", r.Total)
	}
}

func TestSearchPagination(t *testing.T) {
	ix := sample()
	p1, _ := ix.Search(Query{Text: "neon", MaxRating: "teen", Limit: 2})
	p2, _ := ix.Search(Query{Text: "neon", MaxRating: "teen", Limit: 2, Offset: 2})
	if len(p1.Items) != 2 || len(p2.Items) != 1 || p2.Items[0].ID != "r6" || p1.Total != 3 {
		t.Fatalf("%v %v", ids(p1), ids(p2))
	}
}

func TestSearchRejectsInvalid(t *testing.T) {
	ix := sample()
	for _, q := range []Query{
		{Limit: 0}, {Limit: 1000}, {Limit: 1, Offset: -1}, {Limit: 1, MaxRating: "bogus"},
	} {
		if _, err := ix.Search(q); !errors.Is(err, ErrInvalid) {
			t.Fatalf("%+v: %v", q, err)
		}
	}
	if err := ix.Put(Listing{ID: "x", Rating: "bogus"}); !errors.Is(err, ErrInvalid) {
		t.Fatal(err)
	}
}

func TestBlockingRemovesFromResults(t *testing.T) {
	ix := sample()
	if err := ix.Block("r1"); err != nil {
		t.Fatal(err)
	}
	r, _ := ix.Search(Query{Text: "arcade", MaxRating: "mature", Limit: 5})
	if len(r.Items) != 0 {
		t.Fatal(ids(r))
	}
	if !errors.Is(ix.Block("nope"), ErrNotFound) {
		t.Fatal("want not found")
	}
}
