package main

import (
	"context"
	"encoding/json"
	"log"
	"net"
	"net/http"
	"os"
	"os/signal"
	"strings"
	"syscall"
	"time"

	"github.com/oncoursemail/Metallic/services/metallic-control/internal/control"
)

func fatalIfMissing(name string) string {
	v := os.Getenv(name)
	if v == "" {
		log.Fatalf("required environment variable %s is missing", name)
	}
	return v
}
func loopbackAddr(addr string) bool {
	host, _, err := net.SplitHostPort(addr)
	if err != nil {
		return false
	}
	ip := net.ParseIP(host)
	return ip != nil && ip.IsLoopback() || strings.EqualFold(host, "localhost")
}
func main() {
	invPath := fatalIfMissing("METALLIC_INVENTORY_PATH")
	statePath := fatalIfMissing("METALLIC_STATE_PATH")
	admin := fatalIfMissing("METALLIC_ADMIN_TOKEN")
	if len(admin) < 24 {
		log.Fatal("METALLIC_ADMIN_TOKEN must be at least 24 characters")
	}
	var tokens map[string]string
	if err := json.Unmarshal([]byte(fatalIfMissing("METALLIC_AGENT_TOKENS_JSON")), &tokens); err != nil {
		log.Fatal("invalid METALLIC_AGENT_TOKENS_JSON")
	}
	for id, t := range tokens {
		if len(t) < 24 || id == "" {
			log.Fatal("each agent must have a separate nonempty token of at least 24 characters")
		}
	}
	addr := os.Getenv("METALLIC_LISTEN")
	if addr == "" {
		addr = "127.0.0.1:8098"
	}
	if !loopbackAddr(addr) {
		log.Fatal("lab service may only bind to loopback; use authenticated HTTPS reverse proxy on an isolated management network")
	}
	inv, err := control.LoadInventory(invPath)
	if err != nil {
		log.Fatalf("invalid inventory: %v", err)
	}
	ctrl, err := control.New(inv, statePath, nil)
	if err != nil {
		log.Fatalf("refusing to start without valid state: %v", err)
	}
	srv := &http.Server{Addr: addr, Handler: (control.API{Control: ctrl, AdminToken: admin, AgentTokens: tokens}).Handler(), ReadHeaderTimeout: 4 * time.Second, ReadTimeout: 7 * time.Second, WriteTimeout: 7 * time.Second, IdleTimeout: 30 * time.Second, MaxHeaderBytes: 8192}
	stop, done := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer done()
	go func() {
		<-stop.Done()
		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		_ = srv.Shutdown(ctx)
	}()
	log.Printf("Metallic lab control API listening on %s (not production; no game processes launched)", addr)
	if err := srv.ListenAndServe(); err != http.ErrServerClosed {
		log.Fatalf("http server: %v", err)
	}
}
