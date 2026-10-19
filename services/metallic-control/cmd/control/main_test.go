package main

import "testing"

func TestLabBindRequiresLoopback(t *testing.T) {
	for _, tcase := range []struct {
		addr string
		want bool
	}{
		{"127.0.0.1:8098", true}, {"[::1]:8098", true}, {"localhost:8098", true},
		{"0.0.0.0:8098", false}, {"[::]:8098", false}, {"192.0.2.5:8098", false}, {"bad", false},
	} {
		if got := loopbackAddr(tcase.addr); got != tcase.want {
			t.Fatalf("%s got %v want %v", tcase.addr, got, tcase.want)
		}
	}
}
