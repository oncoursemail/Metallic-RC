package control

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"errors"
	"strings"
	"time"
)

// ErrTicket is returned for every join-ticket failure so callers cannot
// distinguish a bad signature from a wrong scope or an expired ticket.
var ErrTicket = errors.New("invalid_ticket")

const (
	minTicketSecret = 32
	maxTicketTTL    = 10 * time.Minute
)

// TicketClaims scope a join ticket to one room, region and client build.
type TicketClaims struct {
	RoomID    string `json:"room"`
	Region    string `json:"region"`
	Build     string `json:"build"`
	ExpiresAt int64  `json:"exp"` // unix seconds
}

var ticketEnc = base64.RawURLEncoding

func ticketMAC(secret []byte, payload string) []byte {
	m := hmac.New(sha256.New, secret)
	m.Write([]byte(payload))
	return m.Sum(nil)
}

// IssueTicket signs claims with an HMAC-SHA256 secret held by the control plane.
func IssueTicket(secret []byte, c TicketClaims, now time.Time) (string, error) {
	exp := time.Unix(c.ExpiresAt, 0)
	if len(secret) < minTicketSecret || !validID(c.RoomID) || !validID(c.Region) || !validID(c.Build) || !exp.After(now) || exp.After(now.Add(maxTicketTTL)) {
		return "", ErrInvalid
	}
	body, err := json.Marshal(c)
	if err != nil {
		return "", err
	}
	payload := ticketEnc.EncodeToString(body)
	return payload + "." + ticketEnc.EncodeToString(ticketMAC(secret, payload)), nil
}

// VerifyTicket checks signature, expiry and that the ticket is scoped to the
// expected room, region and build.
func VerifyTicket(secret []byte, token, roomID, region, build string, now time.Time) (TicketClaims, error) {
	var c TicketClaims
	parts := strings.Split(token, ".")
	if len(secret) < minTicketSecret || len(parts) != 2 {
		return c, ErrTicket
	}
	sig, err := ticketEnc.DecodeString(parts[1])
	if err != nil || !hmac.Equal(sig, ticketMAC(secret, parts[0])) {
		return c, ErrTicket
	}
	body, err := ticketEnc.DecodeString(parts[0])
	if err != nil || json.Unmarshal(body, &c) != nil {
		return TicketClaims{}, ErrTicket
	}
	if !time.Unix(c.ExpiresAt, 0).After(now) || c.RoomID != roomID || c.Region != region || c.Build != build {
		return TicketClaims{}, ErrTicket
	}
	return c, nil
}
