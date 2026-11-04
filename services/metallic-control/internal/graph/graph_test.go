package graph

import (
	"errors"
	"testing"
)

func counterGraph() Graph {
	return Graph{
		Version: Version,
		Nodes: []Node{
			{ID: "start", Type: "event", Key: "tick", Next: "set"},
			{ID: "one", Type: "const", Value: 1},
			{ID: "cur", Type: "get_state", Key: "score"},
			{ID: "sum", Type: "add", A: "cur", B: "one"},
			{ID: "set", Type: "set_state", Key: "score", A: "sum"},
		},
	}
}

func TestRunIsDeterministicAndDoesNotMutateInput(t *testing.T) {
	g := counterGraph()
	in := map[string]int64{"score": 4}
	out, err := Run(g, "tick", in, 100)
	if err != nil {
		t.Fatal(err)
	}
	if out["score"] != 5 || in["score"] != 4 {
		t.Fatalf("out=%v in=%v", out, in)
	}
}

func TestValidateRejectsUntrustedGraphs(t *testing.T) {
	cases := map[string]func(*Graph){
		"bad version":    func(g *Graph) { g.Version = 99 },
		"unknown type":   func(g *Graph) { g.Nodes[1].Type = "exec_shell" },
		"duplicate id":   func(g *Graph) { g.Nodes[1].ID = "start" },
		"empty id":       func(g *Graph) { g.Nodes[1].ID = "" },
		"dangling input": func(g *Graph) { g.Nodes[3].A = "ghost" },
		"dangling next":  func(g *Graph) { g.Nodes[0].Next = "ghost" },
		"flow as data":   func(g *Graph) { g.Nodes[3].A = "set" },
		"data as flow":   func(g *Graph) { g.Nodes[0].Next = "one" },
		"data cycle":     func(g *Graph) { g.Nodes[3].A = "sum" },
		"missing key":    func(g *Graph) { g.Nodes[4].Key = "" },
		"too many nodes": func(g *Graph) { g.Nodes = append(g.Nodes, make([]Node, MaxNodes)...) },
	}
	for name, mutate := range cases {
		g := counterGraph()
		mutate(&g)
		if err := Validate(g); !errors.Is(err, ErrInvalid) {
			t.Errorf("%s: err=%v, want ErrInvalid", name, err)
		}
		if _, err := Run(g, "tick", nil, 100); !errors.Is(err, ErrInvalid) {
			t.Errorf("%s: Run err=%v, want ErrInvalid", name, err)
		}
	}
}

func TestRunStopsCyclicFlowAtQuota(t *testing.T) {
	g := counterGraph()
	g.Nodes[4].Next = "set" // set_state loops on itself forever
	if err := Validate(g); err != nil {
		t.Fatalf("flow loops are valid but quota-limited: %v", err)
	}
	in := map[string]int64{}
	out, err := Run(g, "tick", in, 50)
	if !errors.Is(err, ErrQuota) {
		t.Fatalf("err=%v, want ErrQuota", err)
	}
	if out != nil || len(in) != 0 {
		t.Fatalf("partial state must not leak: out=%v in=%v", out, in)
	}
}

func TestRunUnknownEvent(t *testing.T) {
	if _, err := Run(counterGraph(), "nope", nil, 10); !errors.Is(err, ErrNoEvent) {
		t.Fatalf("err=%v", err)
	}
}
