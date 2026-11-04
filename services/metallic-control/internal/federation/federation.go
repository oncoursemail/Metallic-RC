// Package federation is a local, offline lab model of the opt-in federation
// status exchange: a project publishes an Ed25519-signed, timestamped status
// and only projects that were explicitly registered (opted in) with their
// public key are accepted. Player counts are never inferred: a count exists
// only if the publishing project chose to sign and publish it.
package federation

import (
	"crypto/ed25519"
	"encoding/json"
	"errors"
	"fmt"
	"sync"
	"time"
)

const (
	// Schema is the status payload version.
	Schema = 1
	// DefaultTTL is how long a signed status stays acceptable.
	DefaultTTL = 60 * time.Second
	// MaxClockSkew is how far in the future an observation may be.
	MaxClockSkew = 5 * time.Second
)

var (
	ErrUnknownProject = errors.New("federation: project not registered")
	ErrBadSignature   = errors.New("federation: bad signature")
	ErrStale          = errors.New("federation: stale status")
	ErrFuture         = errors.New("federation: status from the future")
	ErrReplay         = errors.New("federation: replayed or out-of-order status")
	ErrInvalid        = errors.New("federation: invalid status")
)

// Status is the signed payload. PlayerCount is optional and is the project's
// own published aggregate; nil means "not published".
type Status struct {
	Schema      int       `json:"schema"`
	ProjectID   string    `json:"projectId"`
	State       string    `json:"state"`
	PlayerCount *int      `json:"playerCount,omitempty"`
	ObservedAt  time.Time `json:"observedAt"`
	Sequence    uint64    `json:"sequence"`
}

// Envelope carries the exact signed bytes plus the signature.
type Envelope struct {
	Payload   json.RawMessage `json:"payload"`
	Signature []byte          `json:"signature"`
}

// Peer is a project that publishes signed status.
type Peer struct {
	id   string
	key  ed25519.PrivateKey
	now  func() time.Time
	mu   sync.Mutex
	next uint64
}

func NewPeer(id string, key ed25519.PrivateKey, now func() time.Time) *Peer {
	return &Peer{id: id, key: key, now: now}
}

// Publish signs a status stamped with the peer's clock.
func (p *Peer) Publish(state string, playerCount *int) (Envelope, error) {
	if state == "" {
		return Envelope{}, fmt.Errorf("%w: empty state", ErrInvalid)
	}
	if playerCount != nil && *playerCount < 0 {
		return Envelope{}, fmt.Errorf("%w: negative player count", ErrInvalid)
	}
	p.mu.Lock()
	p.next++
	seq := p.next
	p.mu.Unlock()
	payload, err := json.Marshal(Status{Schema: Schema, ProjectID: p.id, State: state, PlayerCount: playerCount, ObservedAt: p.now().UTC(), Sequence: seq})
	if err != nil {
		return Envelope{}, err
	}
	return Envelope{Payload: payload, Signature: ed25519.Sign(p.key, payload)}, nil
}

// Registry holds the public keys of opted-in partner projects.
type Registry struct {
	now  func() time.Time
	mu   sync.Mutex
	keys map[string]ed25519.PublicKey
	last map[string]uint64
}

func NewRegistry(now func() time.Time) *Registry {
	return &Registry{now: now, keys: map[string]ed25519.PublicKey{}, last: map[string]uint64{}}
}

// Register opts a partner project in by its public key.
func (r *Registry) Register(projectID string, key ed25519.PublicKey) error {
	if projectID == "" {
		return fmt.Errorf("%w: empty project id", ErrInvalid)
	}
	if len(key) != ed25519.PublicKeySize {
		return fmt.Errorf("%w: bad public key size", ErrInvalid)
	}
	r.mu.Lock()
	defer r.mu.Unlock()
	r.keys[projectID] = append(ed25519.PublicKey(nil), key...)
	return nil
}

// Verify authenticates an envelope from a registered project and returns its
// status. Unregistered, forged, stale, future and replayed statuses are refused.
func (r *Registry) Verify(env Envelope) (Status, error) {
	var st Status
	if err := json.Unmarshal(env.Payload, &st); err != nil {
		return Status{}, fmt.Errorf("%w: %v", ErrInvalid, err)
	}
	r.mu.Lock()
	defer r.mu.Unlock()
	key, ok := r.keys[st.ProjectID]
	if !ok {
		return Status{}, ErrUnknownProject
	}
	if !ed25519.Verify(key, env.Payload, env.Signature) {
		return Status{}, ErrBadSignature
	}
	if st.Schema != Schema || st.State == "" || (st.PlayerCount != nil && *st.PlayerCount < 0) {
		return Status{}, ErrInvalid
	}
	now := r.now()
	if st.ObservedAt.After(now.Add(MaxClockSkew)) {
		return Status{}, ErrFuture
	}
	if now.Sub(st.ObservedAt) > DefaultTTL {
		return Status{}, ErrStale
	}
	if st.Sequence <= r.last[st.ProjectID] {
		return Status{}, ErrReplay
	}
	r.last[st.ProjectID] = st.Sequence
	return st, nil
}
