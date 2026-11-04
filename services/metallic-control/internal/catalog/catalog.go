// Package catalog implements an in-memory, lab-only searchable listing index
// that only ever returns approved, unblocked content within a rating ceiling.
package catalog

import (
	"errors"
	"sort"
	"strings"
	"sync"
)

var (
	ErrInvalid  = errors.New("invalid_input")
	ErrNotFound = errors.New("not_found")
)

const MaxLimit = 100

var ratingRank = map[string]int{"everyone": 0, "teen": 1, "mature": 2}

type Listing struct {
	ID       string   `json:"id"`
	Name     string   `json:"name"`
	Tags     []string `json:"tags"`
	Approved bool     `json:"approved"`
	Blocked  bool     `json:"blocked"`
	Rating   string   `json:"rating"`
}

type Query struct {
	Text      string
	MaxRating string // empty means "everyone"
	Limit     int
	Offset    int
}

type Page struct {
	Items []Listing `json:"items"`
	Total int       `json:"total"`
}

type Index struct {
	mu       sync.RWMutex
	listings map[string]Listing
}

func New() *Index { return &Index{listings: map[string]Listing{}} }

func (ix *Index) Put(l Listing) error {
	if l.ID == "" {
		return ErrInvalid
	}
	if _, ok := ratingRank[l.Rating]; !ok {
		return ErrInvalid
	}
	l.Tags = append([]string(nil), l.Tags...)
	ix.mu.Lock()
	defer ix.mu.Unlock()
	ix.listings[l.ID] = l
	return nil
}

func (ix *Index) Block(id string) error {
	ix.mu.Lock()
	defer ix.mu.Unlock()
	l, ok := ix.listings[id]
	if !ok {
		return ErrNotFound
	}
	l.Blocked = true
	ix.listings[id] = l
	return nil
}

func matches(l Listing, text string) bool {
	if text == "" || strings.Contains(strings.ToLower(l.Name), text) {
		return true
	}
	for _, t := range l.Tags {
		if strings.Contains(strings.ToLower(t), text) {
			return true
		}
	}
	return false
}

// Search returns matching listings ordered by ID. Unapproved, blocked and
// over-rated listings are never returned or counted.
func (ix *Index) Search(q Query) (Page, error) {
	if q.Limit < 1 || q.Limit > MaxLimit || q.Offset < 0 {
		return Page{}, ErrInvalid
	}
	max := 0
	if q.MaxRating != "" {
		r, ok := ratingRank[q.MaxRating]
		if !ok {
			return Page{}, ErrInvalid
		}
		max = r
	}
	text := strings.ToLower(strings.TrimSpace(q.Text))
	ix.mu.RLock()
	var all []Listing
	for _, l := range ix.listings {
		if l.Approved && !l.Blocked && ratingRank[l.Rating] <= max && matches(l, text) {
			all = append(all, l)
		}
	}
	ix.mu.RUnlock()
	sort.Slice(all, func(i, j int) bool { return all[i].ID < all[j].ID })
	page := Page{Items: []Listing{}, Total: len(all)}
	if q.Offset < len(all) {
		end := q.Offset + q.Limit
		if end > len(all) {
			end = len(all)
		}
		page.Items = all[q.Offset:end]
	}
	return page, nil
}
