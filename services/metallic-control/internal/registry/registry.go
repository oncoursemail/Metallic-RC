// Package registry verifies signed per-project launcher manifests and gives each
// registered project its own installation directory. It does not download or
// launch anything.
package registry

import (
	"crypto/ed25519"
	"encoding/json"
	"errors"
	"path/filepath"
	"regexp"
	"sync"
)

var (
	ErrInvalid      = errors.New("invalid_input")
	ErrBadSignature = errors.New("bad_signature")
	ErrNotFound     = errors.New("not_found")
)

var projectID = regexp.MustCompile(`^[a-z0-9][a-z0-9_-]{0,63}$`)

// Manifest describes one supported external project.
type Manifest struct {
	ID           string `json:"id"`
	Name         string `json:"name"`
	Version      string `json:"version"`
	InstallerURL string `json:"installerUrl"`
	SHA256       string `json:"sha256"`
}

// Signed is a manifest with an Ed25519 signature over its canonical JSON.
type Signed struct {
	Manifest  Manifest `json:"manifest"`
	Signature []byte   `json:"signature"`
}

// Sign signs the manifest with the publisher's private key.
func Sign(key ed25519.PrivateKey, m Manifest) (Signed, error) {
	payload, err := json.Marshal(m)
	if err != nil {
		return Signed{}, err
	}
	return Signed{Manifest: m, Signature: ed25519.Sign(key, payload)}, nil
}

// Registry holds verified manifests; installs live in root/<project id>.
type Registry struct {
	root string
	pub  ed25519.PublicKey

	mu       sync.RWMutex
	projects map[string]Manifest
}

func New(root string, pub ed25519.PublicKey) *Registry {
	return &Registry{root: root, pub: pub, projects: map[string]Manifest{}}
}

// Register adds a manifest only if its signature verifies and its ID is safe.
func (r *Registry) Register(s Signed) error {
	if len(r.pub) != ed25519.PublicKeySize || len(s.Signature) != ed25519.SignatureSize {
		return ErrBadSignature
	}
	payload, err := json.Marshal(s.Manifest)
	if err != nil {
		return ErrInvalid
	}
	if !ed25519.Verify(r.pub, payload, s.Signature) {
		return ErrBadSignature
	}
	if !projectID.MatchString(s.Manifest.ID) {
		return ErrInvalid
	}
	r.mu.Lock()
	defer r.mu.Unlock()
	r.projects[s.Manifest.ID] = s.Manifest
	return nil
}

func (r *Registry) Get(id string) (Manifest, bool) {
	r.mu.RLock()
	defer r.mu.RUnlock()
	m, ok := r.projects[id]
	return m, ok
}

// InstallDir returns the project's private installation directory.
func (r *Registry) InstallDir(id string) (string, error) {
	if _, ok := r.Get(id); !ok {
		return "", ErrNotFound
	}
	return filepath.Join(r.root, id), nil
}
