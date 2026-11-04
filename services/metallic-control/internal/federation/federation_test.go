package federation

import (
	"crypto/ed25519"
	"crypto/rand"
	"encoding/json"
	"errors"
	"testing"
	"time"
)

var t0 = time.Date(2026, 11, 4, 12, 0, 0, 0, time.UTC)

func newPeer(t *testing.T, id string, now *time.Time) (*Peer, ed25519.PublicKey) {
	t.Helper()
	pub, priv, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	return NewPeer(id, priv, func() time.Time { return *now }), pub
}

func count(n int) *int { return &n }

func mustPublish(t *testing.T, p *Peer, c *int) Envelope {
	t.Helper()
	env, err := p.Publish("online", c)
	if err != nil {
		t.Fatal(err)
	}
	return env
}

func TestTwoProjectsExchangeSignedStatusAndTimestamps(t *testing.T) {
	now := t0
	a, pubA := newPeer(t, "project-a", &now)
	b, pubB := newPeer(t, "project-b", &now)
	regA := NewRegistry(func() time.Time { return now })
	regB := NewRegistry(func() time.Time { return now })
	if err := regA.Register("project-b", pubB); err != nil {
		t.Fatal(err)
	}
	if err := regB.Register("project-a", pubA); err != nil {
		t.Fatal(err)
	}

	got, err := regB.Verify(mustPublish(t, a, count(12)))
	if err != nil {
		t.Fatal(err)
	}
	if got.ProjectID != "project-a" || got.State != "online" || got.PlayerCount == nil || *got.PlayerCount != 12 || !got.ObservedAt.Equal(t0) {
		t.Fatalf("unexpected status %+v", got)
	}

	now = now.Add(5 * time.Second)
	envB, err := b.Publish("degraded", nil)
	if err != nil {
		t.Fatal(err)
	}
	got, err = regA.Verify(envB)
	if err != nil {
		t.Fatal(err)
	}
	if got.State != "degraded" || got.PlayerCount != nil || !got.ObservedAt.Equal(t0.Add(5*time.Second)) {
		t.Fatalf("unexpected status %+v", got)
	}
}

func TestVerifyRejections(t *testing.T) {
	now := t0
	a, pubA := newPeer(t, "project-a", &now)
	reg := NewRegistry(func() time.Time { return now })
	if _, err := reg.Verify(mustPublish(t, a, count(1))); !errors.Is(err, ErrUnknownProject) {
		t.Fatalf("unregistered project must be rejected, got %v", err)
	}
	if err := reg.Register("project-a", pubA); err != nil {
		t.Fatal(err)
	}

	env := mustPublish(t, a, count(1))
	tampered := env
	tampered.Payload = json.RawMessage(string(env.Payload[:len(env.Payload)-1]) + `,"x":1}`)
	if _, err := reg.Verify(tampered); !errors.Is(err, ErrBadSignature) {
		t.Fatalf("tampered payload: got %v", err)
	}

	other, _ := newPeer(t, "project-a", &now) // same id, different key
	if _, err := reg.Verify(mustPublish(t, other, count(1))); !errors.Is(err, ErrBadSignature) {
		t.Fatalf("wrong key: got %v", err)
	}

	if _, err := reg.Verify(env); err != nil {
		t.Fatal(err)
	}
	if _, err := reg.Verify(env); !errors.Is(err, ErrReplay) {
		t.Fatalf("replay: got %v", err)
	}

	stale := mustPublish(t, a, count(2))
	now = now.Add(DefaultTTL + time.Second)
	if _, err := reg.Verify(stale); !errors.Is(err, ErrStale) {
		t.Fatalf("stale: got %v", err)
	}

	now = t0
	future := mustPublish(t, a, count(3))
	now = t0.Add(-MaxClockSkew - time.Second)
	if _, err := reg.Verify(future); !errors.Is(err, ErrFuture) {
		t.Fatalf("future: got %v", err)
	}
}

func TestPublishRefusesInvalidStatus(t *testing.T) {
	now := t0
	a, _ := newPeer(t, "project-a", &now)
	if _, err := a.Publish("online", count(-1)); err == nil {
		t.Fatal("negative count must be refused")
	}
	if _, err := a.Publish("", nil); err == nil {
		t.Fatal("empty state must be refused")
	}
}

func TestRegistryOnlyHoldsValidOptIns(t *testing.T) {
	reg := NewRegistry(time.Now)
	if err := reg.Register("", make([]byte, ed25519.PublicKeySize)); err == nil {
		t.Fatal("empty id must be refused")
	}
	if err := reg.Register("p", []byte("short")); err == nil {
		t.Fatal("bad key must be refused")
	}
}
