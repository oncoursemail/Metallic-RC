package achievements

import (
	"errors"
	"sync"
	"testing"
	"time"
)

var testNow = time.Date(2026, 11, 4, 12, 0, 0, 0, time.UTC)

func newTestService() (*Service, *Signer) {
	signer := NewSigner([]byte("unit-test-signing-material"))
	defs := []Definition{
		{ID: "first-golf-score", Version: 1},
		{ID: "secret-room", Version: 2, Private: true},
	}
	return NewService(signer, defs, func() time.Time { return testNow }), signer
}

func golfEvent() Event {
	return Event{ID: "evt-1", UserID: "u1", AchievementID: "first-golf-score", DefinitionVersion: 1, Source: SourceServerSession, OccurredAt: testNow}
}

func TestAwardSignedEventOnce(t *testing.T) {
	svc, signer := newTestService()
	ev := golfEvent()
	sig := signer.Sign(ev)
	a, created, err := svc.Award(ev, sig)
	if err != nil || !created {
		t.Fatalf("expected new award, got created=%v err=%v", created, err)
	}
	if a.Proof.Event != ev || a.Proof.Signature != sig {
		t.Fatalf("proof not retained: %+v", a.Proof)
	}
	a2, created, err := svc.Award(ev, sig)
	if err != nil || created || a2 != a {
		t.Fatalf("redelivery must be idempotent: %v %v %v", a2, created, err)
	}
	late := ev
	late.ID = "evt-2"
	late.OccurredAt = testNow.Add(-time.Hour)
	_, created, err = svc.Award(late, signer.Sign(late))
	if err != nil || created {
		t.Fatalf("late event double-awarded: %v %v", created, err)
	}
	if got := svc.AwardsFor("u1"); len(got) != 1 {
		t.Fatalf("want 1 award, got %d", len(got))
	}
}

func TestForgedAndUntrustedEventsDenied(t *testing.T) {
	svc, signer := newTestService()
	ev := golfEvent()
	if _, _, err := svc.Award(ev, "deadbeef"); !errors.Is(err, ErrBadSignature) {
		t.Fatalf("bad sig: %v", err)
	}
	other := NewSigner([]byte("other"))
	if _, _, err := svc.Award(ev, other.Sign(ev)); !errors.Is(err, ErrBadSignature) {
		t.Fatalf("wrong key: %v", err)
	}
	tampered := ev
	sig := signer.Sign(ev)
	tampered.UserID = "u2"
	if _, _, err := svc.Award(tampered, sig); !errors.Is(err, ErrBadSignature) {
		t.Fatalf("tampered: %v", err)
	}
	for _, src := range []string{"client", "synthetic", "testbot", ""} {
		e := ev
		e.Source = src
		if _, _, err := svc.Award(e, signer.Sign(e)); !errors.Is(err, ErrUntrustedSource) {
			t.Fatalf("source %q: %v", src, err)
		}
	}
	unknown := ev
	unknown.AchievementID = "nope"
	if _, _, err := svc.Award(unknown, signer.Sign(unknown)); !errors.Is(err, ErrUnknownDefinition) {
		t.Fatalf("unknown: %v", err)
	}
	wrongVer := ev
	wrongVer.DefinitionVersion = 9
	if _, _, err := svc.Award(wrongVer, signer.Sign(wrongVer)); !errors.Is(err, ErrUnknownDefinition) {
		t.Fatalf("version: %v", err)
	}
	if len(svc.AwardsFor("u1"))+len(svc.AwardsFor("u2")) != 0 {
		t.Fatal("denied events produced awards")
	}
	if n := len(svc.Audit()); n != 9 {
		t.Fatalf("expected 9 audit denials, got %d", n)
	}
}

func TestEventIDReuseForDifferentUserRejected(t *testing.T) {
	svc, signer := newTestService()
	ev := golfEvent()
	if _, _, err := svc.Award(ev, signer.Sign(ev)); err != nil {
		t.Fatal(err)
	}
	e2 := ev
	e2.UserID = "u2"
	if _, _, err := svc.Award(e2, signer.Sign(e2)); !errors.Is(err, ErrEventConflict) {
		t.Fatalf("want conflict: %v", err)
	}
}

func TestPrivateAchievementsHiddenFromPublicView(t *testing.T) {
	svc, signer := newTestService()
	a := golfEvent()
	b := Event{ID: "evt-p", UserID: "u1", AchievementID: "secret-room", DefinitionVersion: 2, Source: SourceServerSession, OccurredAt: testNow}
	for _, e := range []Event{a, b} {
		if _, _, err := svc.Award(e, signer.Sign(e)); err != nil {
			t.Fatal(err)
		}
	}
	if len(svc.AwardsFor("u1")) != 2 {
		t.Fatal("owner should see both")
	}
	pub := svc.PublicAwardsFor("u1")
	if len(pub) != 1 || pub[0].AchievementID != "first-golf-score" {
		t.Fatalf("public view leaked: %+v", pub)
	}
}

func TestConcurrentRedeliveryAwardsOnce(t *testing.T) {
	svc, signer := newTestService()
	ev := golfEvent()
	sig := signer.Sign(ev)
	var wg sync.WaitGroup
	var mu sync.Mutex
	created := 0
	for i := 0; i < 20; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			if _, c, err := svc.Award(ev, sig); err == nil && c {
				mu.Lock()
				created++
				mu.Unlock()
			}
		}()
	}
	wg.Wait()
	if created != 1 {
		t.Fatalf("created %d times", created)
	}
}
