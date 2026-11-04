// Package graph is a minimal Circuits V3 typed graph runtime: a versioned
// schema, strict validation of untrusted graphs, and deterministic execution
// of events, integer math and state under an instruction quota.
package graph

import (
	"errors"
	"fmt"
)

// Version is the only graph schema version this runtime accepts.
const Version = 1

// MaxNodes bounds the size of an untrusted graph.
const MaxNodes = 1024

var (
	ErrInvalid = errors.New("invalid graph")
	ErrQuota   = errors.New("instruction quota exceeded")
	ErrNoEvent = errors.New("event not found")
)

// Node is one graph node. Data nodes (const, get_state, add, mul) produce an
// integer; flow nodes (event, set_state) are chained through Next.
type Node struct {
	ID    string `json:"id"`
	Type  string `json:"type"`
	Key   string `json:"key,omitempty"`   // event name or state key
	Value int64  `json:"value,omitempty"` // const value
	A     string `json:"a,omitempty"`     // data input node id
	B     string `json:"b,omitempty"`     // data input node id
	Next  string `json:"next,omitempty"`  // next flow node id
}

// Graph is a versioned node list.
type Graph struct {
	Version int    `json:"version"`
	Nodes   []Node `json:"nodes"`
}

type spec struct {
	flow   bool
	key    bool
	inputs int
}

var specs = map[string]spec{
	"event":     {flow: true, key: true},
	"set_state": {flow: true, key: true, inputs: 1},
	"const":     {},
	"get_state": {key: true},
	"add":       {inputs: 2},
	"mul":       {inputs: 2},
}

func invalid(format string, args ...any) error {
	return fmt.Errorf("%w: %s", ErrInvalid, fmt.Sprintf(format, args...))
}

// Validate rejects graphs with an unknown version or node type, duplicate or
// dangling ids, mistyped links, or cycles among data dependencies. Cycles in
// flow links are allowed here and bounded at run time by the quota.
func Validate(g Graph) error {
	if g.Version != Version {
		return invalid("unsupported version %d", g.Version)
	}
	if len(g.Nodes) > MaxNodes {
		return invalid("more than %d nodes", MaxNodes)
	}
	byID := make(map[string]*Node, len(g.Nodes))
	for i := range g.Nodes {
		n := &g.Nodes[i]
		if n.ID == "" {
			return invalid("node %d has no id", i)
		}
		if _, dup := byID[n.ID]; dup {
			return invalid("duplicate id %q", n.ID)
		}
		if _, ok := specs[n.Type]; !ok {
			return invalid("node %q has unknown type %q", n.ID, n.Type)
		}
		byID[n.ID] = n
	}
	for i := range g.Nodes {
		n := &g.Nodes[i]
		s := specs[n.Type]
		if s.key && n.Key == "" {
			return invalid("node %q needs a key", n.ID)
		}
		ins := []string{n.A, n.B}
		for j, in := range ins {
			if j >= s.inputs {
				if in != "" {
					return invalid("node %q has an unexpected input", n.ID)
				}
				continue
			}
			t, ok := byID[in]
			if !ok || specs[t.Type].flow {
				return invalid("node %q input %q is not a data node", n.ID, in)
			}
		}
		if n.Next != "" {
			t, ok := byID[n.Next]
			if !s.flow || !ok || !specs[t.Type].flow || t.Type == "event" {
				return invalid("node %q has a bad next link %q", n.ID, n.Next)
			}
		}
	}
	// Data-dependency cycle detection (0 unseen, 1 in progress, 2 done).
	state := make(map[string]int, len(byID))
	var visit func(id string) bool
	visit = func(id string) bool {
		switch state[id] {
		case 1:
			return false
		case 2:
			return true
		}
		state[id] = 1
		n := byID[id]
		for _, in := range []string{n.A, n.B} {
			if in != "" && !visit(in) {
				return false
			}
		}
		state[id] = 2
		return true
	}
	for i := range g.Nodes {
		if !visit(g.Nodes[i].ID) {
			return invalid("data dependency cycle at %q", g.Nodes[i].ID)
		}
	}
	return nil
}

// Run validates g, then executes the event node named event. Every node visit
// and data evaluation costs one instruction; exceeding quota aborts with
// ErrQuota and discards all state changes. state is never modified; the new
// state is returned.
func Run(g Graph, event string, state map[string]int64, quota int) (map[string]int64, error) {
	if err := Validate(g); err != nil {
		return nil, err
	}
	byID := make(map[string]*Node, len(g.Nodes))
	var start *Node
	for i := range g.Nodes {
		n := &g.Nodes[i]
		byID[n.ID] = n
		if start == nil && n.Type == "event" && n.Key == event {
			start = n
		}
	}
	if start == nil {
		return nil, fmt.Errorf("%w: %q", ErrNoEvent, event)
	}
	out := make(map[string]int64, len(state))
	for k, v := range state {
		out[k] = v
	}
	used := 0
	spend := func() error {
		used++
		if used > quota {
			return ErrQuota
		}
		return nil
	}
	var eval func(n *Node) (int64, error)
	eval = func(n *Node) (int64, error) {
		if err := spend(); err != nil {
			return 0, err
		}
		switch n.Type {
		case "const":
			return n.Value, nil
		case "get_state":
			return out[n.Key], nil
		}
		a, err := eval(byID[n.A])
		if err != nil {
			return 0, err
		}
		b, err := eval(byID[n.B])
		if err != nil {
			return 0, err
		}
		if n.Type == "mul" {
			return a * b, nil
		}
		return a + b, nil
	}
	for cur := start; cur != nil; {
		if err := spend(); err != nil {
			return nil, err
		}
		if cur.Type == "set_state" {
			v, err := eval(byID[cur.A])
			if err != nil {
				return nil, err
			}
			out[cur.Key] = v
		}
		cur = byID[cur.Next]
	}
	return out, nil
}
