package registry

import (
	"crypto/ed25519"
	"crypto/rand"
	"errors"
	"path/filepath"
	"strings"
	"testing"
)

func testKeys(t *testing.T) (ed25519.PublicKey, ed25519.PrivateKey) {
	t.Helper()
	pub, priv, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	return pub, priv
}

func manifest(id string) Manifest {
	return Manifest{ID: id, Name: "Game " + id, Version: "1.0.0", InstallerURL: "https://example.invalid/" + id + ".zip", SHA256: strings.Repeat("a", 64)}
}

func TestSignedManifestAccepted(t *testing.T) {
	pub, priv := testKeys(t)
	reg := New(t.TempDir(), pub)
	s, err := Sign(priv, manifest("alpha"))
	if err != nil {
		t.Fatal(err)
	}
	if err := reg.Register(s); err != nil {
		t.Fatalf("valid manifest rejected: %v", err)
	}
	if _, ok := reg.Get("alpha"); !ok {
		t.Fatal("registered project missing")
	}
}

func TestTamperedManifestRejected(t *testing.T) {
	pub, priv := testKeys(t)
	reg := New(t.TempDir(), pub)
	s, _ := Sign(priv, manifest("alpha"))
	s.Manifest.InstallerURL = "https://evil.invalid/alpha.zip"
	if err := reg.Register(s); !errors.Is(err, ErrBadSignature) {
		t.Fatalf("want ErrBadSignature, got %v", err)
	}
	if _, ok := reg.Get("alpha"); ok {
		t.Fatal("tampered manifest was registered")
	}
}

func TestWrongKeyRejected(t *testing.T) {
	pub, _ := testKeys(t)
	_, otherPriv := testKeys(t)
	reg := New(t.TempDir(), pub)
	s, _ := Sign(otherPriv, manifest("alpha"))
	if err := reg.Register(s); !errors.Is(err, ErrBadSignature) {
		t.Fatalf("want ErrBadSignature, got %v", err)
	}
}

func TestInstallDirsIsolated(t *testing.T) {
	pub, priv := testKeys(t)
	root := t.TempDir()
	reg := New(root, pub)
	for _, id := range []string{"alpha", "beta"} {
		s, _ := Sign(priv, manifest(id))
		if err := reg.Register(s); err != nil {
			t.Fatal(err)
		}
	}
	a, err := reg.InstallDir("alpha")
	if err != nil {
		t.Fatal(err)
	}
	b, _ := reg.InstallDir("beta")
	if a == b || a != filepath.Join(root, "alpha") {
		t.Fatalf("dirs not isolated: %q %q", a, b)
	}
	if _, err := reg.InstallDir("missing"); !errors.Is(err, ErrNotFound) {
		t.Fatalf("want ErrNotFound, got %v", err)
	}
}

func TestUnsafeProjectIDRejected(t *testing.T) {
	pub, priv := testKeys(t)
	reg := New(t.TempDir(), pub)
	for _, id := range []string{"../escape", "a/b", "", ".", "..", `a\b`} {
		s, _ := Sign(priv, manifest(id))
		if err := reg.Register(s); !errors.Is(err, ErrInvalid) {
			t.Fatalf("id %q: want ErrInvalid, got %v", id, err)
		}
	}
}
