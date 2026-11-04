// Package achievements is a local-only, in-memory lab of the trusted award
// path: server-signed events, immutable proof, idempotent awards and audit.
// It has no persistence or network surface yet.
package achievements

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"sync"
	"time"
)

// SourceServerSession is the only event source allowed to award.
const SourceServerSession = "server-session"

var (
	ErrBadSignature      = errors.New("achievements: invalid event signature")
	ErrUntrustedSource   = errors.New("achievements: untrusted event source")
	ErrUnknownDefinition = errors.New("achievements: unknown achievement definition or version")
	ErrEventConflict     = errors.New("achievements: event id already used for a different event")
)

// Definition is a versioned achievement definition.
type Definition struct {
	ID      string
	Version int
	Private bool
}

// Event is a server-issued progression event.
type Event struct {
	ID                string
	UserID            string
	AchievementID     string
	DefinitionVersion int
	Source            string
	OccurredAt        time.Time
}

// Signer produces and verifies server signatures over events.
type Signer struct{ key []byte }

func NewSigner(key []byte) *Signer { return &Signer{key: append([]byte(nil), key...)} }

func (s *Signer) mac(e Event) []byte {
	m := hmac.New(sha256.New, s.key)
	for _, f := range []string{e.ID, e.UserID, e.AchievementID, fmt.Sprint(e.DefinitionVersion), e.Source, e.OccurredAt.UTC().Format(time.RFC3339Nano)} {
		fmt.Fprintf(m, "%d:%s;", len(f), f)
	}
	return m.Sum(nil)
}

func (s *Signer) Sign(e Event) string { return hex.EncodeToString(s.mac(e)) }

func (s *Signer) Verify(e Event, sig string) bool {
	got, err := hex.DecodeString(sig)
	if err != nil {
		return false
	}
	return hmac.Equal(got, s.mac(e))
}

// Proof is the immutable evidence stored with an award.
type Proof struct {
	Event     Event
	Signature string
}

// Award is a granted achievement.
type Award struct {
	UserID        string
	AchievementID string
	Version       int
	Private       bool
	AwardedAt     time.Time
	Proof         Proof
}

// AuditRecord notes a denied or redelivered event.
type AuditRecord struct {
	At      time.Time
	EventID string
	UserID  string
	Outcome string
}

type awardKey struct{ user, achievement string }

// Service is the idempotent award service.
type Service struct {
	mu     sync.Mutex
	signer *Signer
	defs   map[awardKey]Definition // keyed by achievement id + version string
	now    func() time.Time
	events map[string]Event
	awards map[awardKey]Award
	order  []awardKey
	audit  []AuditRecord
}

func defKey(id string, version int) awardKey {
	return awardKey{user: fmt.Sprint(version), achievement: id}
}

func NewService(signer *Signer, defs []Definition, now func() time.Time) *Service {
	s := &Service{signer: signer, defs: map[awardKey]Definition{}, now: now, events: map[string]Event{}, awards: map[awardKey]Award{}}
	for _, d := range defs {
		s.defs[defKey(d.ID, d.Version)] = d
	}
	return s
}

func (s *Service) deny(e Event, outcome string, err error) (Award, bool, error) {
	s.audit = append(s.audit, AuditRecord{At: s.now(), EventID: e.ID, UserID: e.UserID, Outcome: outcome})
	return Award{}, false, err
}

// Award grants the achievement for a signed server event. created is false
// when the event is a redelivery or the user already holds the achievement.
func (s *Service) Award(e Event, sig string) (Award, bool, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if !s.signer.Verify(e, sig) {
		return s.deny(e, "denied:bad-signature", ErrBadSignature)
	}
	if e.Source != SourceServerSession {
		return s.deny(e, "denied:untrusted-source", ErrUntrustedSource)
	}
	def, ok := s.defs[defKey(e.AchievementID, e.DefinitionVersion)]
	if !ok {
		return s.deny(e, "denied:unknown-definition", ErrUnknownDefinition)
	}
	if prev, seen := s.events[e.ID]; seen && prev != e {
		return s.deny(e, "denied:event-conflict", ErrEventConflict)
	}
	s.events[e.ID] = e
	k := awardKey{e.UserID, e.AchievementID}
	if existing, held := s.awards[k]; held {
		s.audit = append(s.audit, AuditRecord{At: s.now(), EventID: e.ID, UserID: e.UserID, Outcome: "duplicate"})
		return existing, false, nil
	}
	a := Award{UserID: e.UserID, AchievementID: e.AchievementID, Version: def.Version, Private: def.Private, AwardedAt: s.now(), Proof: Proof{Event: e, Signature: sig}}
	s.awards[k] = a
	s.order = append(s.order, k)
	s.audit = append(s.audit, AuditRecord{At: a.AwardedAt, EventID: e.ID, UserID: e.UserID, Outcome: "awarded"})
	return a, true, nil
}

// AwardsFor returns every award of a user (owner view).
func (s *Service) AwardsFor(user string) []Award { return s.collect(user, true) }

// PublicAwardsFor omits private achievements.
func (s *Service) PublicAwardsFor(user string) []Award { return s.collect(user, false) }

func (s *Service) collect(user string, includePrivate bool) []Award {
	s.mu.Lock()
	defer s.mu.Unlock()
	var out []Award
	for _, k := range s.order {
		if k.user != user {
			continue
		}
		if a := s.awards[k]; includePrivate || !a.Private {
			out = append(out, a)
		}
	}
	return out
}

// Audit returns a copy of the audit trail.
func (s *Service) Audit() []AuditRecord {
	s.mu.Lock()
	defer s.mu.Unlock()
	return append([]AuditRecord(nil), s.audit...)
}
