package ledger

import (
	"errors"
	"fmt"
	"sync"
	"testing"
)

func TestConcurrentDebitsNeverOverspend(t *testing.T) {
	l := New()
	if _, err := l.Credit("alice", "seed", 100); err != nil {
		t.Fatal(err)
	}
	var wg sync.WaitGroup
	var mu sync.Mutex
	ok, insufficient := 0, 0
	for i := 0; i < 50; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			_, err := l.Debit("alice", fmt.Sprintf("d%d", i), 10)
			mu.Lock()
			defer mu.Unlock()
			switch {
			case err == nil:
				ok++
			case errors.Is(err, ErrInsufficient):
				insufficient++
			default:
				t.Errorf("unexpected error: %v", err)
			}
		}(i)
	}
	wg.Wait()
	if ok != 10 || insufficient != 40 {
		t.Fatalf("ok=%d insufficient=%d", ok, insufficient)
	}
	if b := l.Balance("alice"); b != 0 {
		t.Fatalf("balance %d", b)
	}
}

func TestIdempotentReplayAppliesOnce(t *testing.T) {
	l := New()
	for i := 0; i < 3; i++ {
		b, err := l.Credit("bob", "k1", 25)
		if err != nil || b != 25 {
			t.Fatalf("credit %d: %d %v", i, b, err)
		}
	}
	if _, err := l.Debit("bob", "k2", 5); err != nil {
		t.Fatal(err)
	}
	if b, err := l.Debit("bob", "k2", 5); err != nil || b != 20 {
		t.Fatalf("replay: %d %v", b, err)
	}
	if b := l.Balance("bob"); b != 20 {
		t.Fatalf("balance %d", b)
	}
}

func TestIdempotencyKeyReuseWithDifferentRequestConflicts(t *testing.T) {
	l := New()
	if _, err := l.Credit("c", "k", 10); err != nil {
		t.Fatal(err)
	}
	if _, err := l.Credit("c", "k", 11); !errors.Is(err, ErrConflict) {
		t.Fatalf("want conflict, got %v", err)
	}
	if _, err := l.Debit("c", "k", 10); !errors.Is(err, ErrConflict) {
		t.Fatalf("want conflict, got %v", err)
	}
}

func TestInvalidInput(t *testing.T) {
	l := New()
	for _, c := range []struct {
		acct, key string
		amt       int64
	}{{"", "k", 1}, {"a", "", 1}, {"a", "k", 0}, {"a", "k", -5}} {
		if _, err := l.Credit(c.acct, c.key, c.amt); !errors.Is(err, ErrInvalid) {
			t.Fatalf("%+v: %v", c, err)
		}
	}
}

func TestRejectedDebitIsReplayedAsRejectedWithoutChangingBalance(t *testing.T) {
	l := New()
	if _, err := l.Debit("d", "k", 5); !errors.Is(err, ErrInsufficient) {
		t.Fatalf("got %v", err)
	}
	if _, err := l.Credit("d", "fund", 100); err != nil {
		t.Fatal(err)
	}
	if _, err := l.Debit("d", "k", 5); !errors.Is(err, ErrInsufficient) {
		t.Fatalf("got %v", err)
	}
	if b := l.Balance("d"); b != 100 {
		t.Fatalf("balance %d", b)
	}
}

func TestOverflowRejected(t *testing.T) {
	l := New()
	if _, err := l.Credit("o", "a", 1<<62); err != nil {
		t.Fatal(err)
	}
	if _, err := l.Credit("o", "b", 1<<62); !errors.Is(err, ErrInvalid) {
		t.Fatalf("got %v", err)
	}
}
