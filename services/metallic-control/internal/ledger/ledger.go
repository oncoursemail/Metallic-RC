// Package ledger implements a lab-only, in-memory virtual token ledger.
// Credits and debits are atomic and idempotent per key, and balances can never
// go negative. It has no persistence and does not handle real money.
package ledger

import (
	"errors"
	"math"
	"sync"
)

var (
	ErrInvalid      = errors.New("invalid_input")
	ErrInsufficient = errors.New("insufficient_balance")
	ErrConflict     = errors.New("conflict")
)

type entry struct {
	account string
	credit  bool
	amount  int64
	balance int64
	err     error
}

// Ledger serialises all mutations behind one mutex.
type Ledger struct {
	mu       sync.Mutex
	balances map[string]int64
	entries  map[string]entry
}

func New() *Ledger {
	return &Ledger{balances: map[string]int64{}, entries: map[string]entry{}}
}

// Balance returns the current balance of an account (zero if unknown).
func (l *Ledger) Balance(account string) int64 {
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.balances[account]
}

// Credit adds amount tokens once per idempotency key and returns the balance.
func (l *Ledger) Credit(account, key string, amount int64) (int64, error) {
	return l.apply(account, key, amount, true)
}

// Debit removes amount tokens once per idempotency key, or fails with
// ErrInsufficient without changing the balance.
func (l *Ledger) Debit(account, key string, amount int64) (int64, error) {
	return l.apply(account, key, amount, false)
}

func (l *Ledger) apply(account, key string, amount int64, credit bool) (int64, error) {
	if account == "" || key == "" || amount <= 0 {
		return 0, ErrInvalid
	}
	l.mu.Lock()
	defer l.mu.Unlock()
	if prev, ok := l.entries[key]; ok {
		if prev.account != account || prev.credit != credit || prev.amount != amount {
			return 0, ErrConflict
		}
		return prev.balance, prev.err
	}
	cur := l.balances[account]
	e := entry{account: account, credit: credit, amount: amount}
	switch {
	case credit && cur > math.MaxInt64-amount:
		return 0, ErrInvalid
	case credit:
		cur += amount
	case cur < amount:
		e.err = ErrInsufficient
	default:
		cur -= amount
	}
	l.balances[account] = cur
	e.balance = cur
	l.entries[key] = e
	return e.balance, e.err
}
