package control

import (
	"strings"
	"testing"
	"time"
)

var ticketSecret = []byte("0123456789abcdef0123456789abcdef")

func ticketClaims(now time.Time) TicketClaims {
	return TicketClaims{RoomID: "room-1", Region: "us-test", Build: "build-42", ExpiresAt: now.Add(time.Minute).Unix()}
}

func TestTicketRoundTripAndScope(t *testing.T) {
	now := time.Date(2026, 11, 4, 12, 0, 0, 0, time.UTC)
	tok, err := IssueTicket(ticketSecret, ticketClaims(now), now)
	if err != nil {
		t.Fatal(err)
	}
	got, err := VerifyTicket(ticketSecret, tok, "room-1", "us-test", "build-42", now)
	if err != nil || got.RoomID != "room-1" {
		t.Fatalf("%v %+v", err, got)
	}
	for _, c := range [][3]string{{"room-2", "us-test", "build-42"}, {"room-1", "eu-test", "build-42"}, {"room-1", "us-test", "build-43"}} {
		if _, err = VerifyTicket(ticketSecret, tok, c[0], c[1], c[2], now); err != ErrTicket {
			t.Fatalf("scope %v accepted: %v", c, err)
		}
	}
}

func TestTicketRejectsExpiredTamperedAndWrongKey(t *testing.T) {
	now := time.Date(2026, 11, 4, 12, 0, 0, 0, time.UTC)
	tok, err := IssueTicket(ticketSecret, ticketClaims(now), now)
	if err != nil {
		t.Fatal(err)
	}
	if _, err = VerifyTicket(ticketSecret, tok, "room-1", "us-test", "build-42", now.Add(time.Minute)); err != ErrTicket {
		t.Fatalf("expired accepted: %v", err)
	}
	parts := strings.Split(tok, ".")
	bad := parts[0] + "." + parts[1][:len(parts[1])-2] + "AA"
	for _, s := range []string{bad, "", "x", tok + ".y"} {
		if _, err = VerifyTicket(ticketSecret, s, "room-1", "us-test", "build-42", now); err != ErrTicket {
			t.Fatalf("%q accepted: %v", s, err)
		}
	}
	other := []byte("fedcba9876543210fedcba9876543210")
	if _, err = VerifyTicket(other, tok, "room-1", "us-test", "build-42", now); err != ErrTicket {
		t.Fatalf("wrong key accepted: %v", err)
	}
}

func TestTicketIssueValidation(t *testing.T) {
	now := time.Date(2026, 11, 4, 12, 0, 0, 0, time.UTC)
	if _, err := IssueTicket([]byte("short"), ticketClaims(now), now); err != ErrInvalid {
		t.Fatalf("short secret: %v", err)
	}
	c := ticketClaims(now)
	c.Build = "bad build"
	if _, err := IssueTicket(ticketSecret, c, now); err != ErrInvalid {
		t.Fatalf("bad build: %v", err)
	}
	c = ticketClaims(now)
	c.ExpiresAt = now.Add(time.Hour).Unix()
	if _, err := IssueTicket(ticketSecret, c, now); err != ErrInvalid {
		t.Fatalf("too long lived: %v", err)
	}
	c.ExpiresAt = now.Unix()
	if _, err := IssueTicket(ticketSecret, c, now); err != ErrInvalid {
		t.Fatalf("already expired: %v", err)
	}
}
