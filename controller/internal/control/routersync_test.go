package control

import (
	"context"
	"errors"
	"testing"

	"github.com/libp2p/go-libp2p/core/peer"
)

type neighborWriterFunc func(context.Context, string, string) error

func (f neighborWriterFunc) UpsertNeighbor(
	ctx context.Context,
	id string,
	address string,
) error {
	return f(ctx, id, address)
}

func TestSyncRouterRetainsMetadataAfterFailure(t *testing.T) {
	// Internal identity fixture; authentication is tested separately.
	id := peer.ID("remote-fixture")
	info := PeerInfo{
		Version:    1,
		UDPAddress: "127.0.0.1:5002",
	}

	controller := &NetworkingController{
		peerInfo: map[peer.ID]PeerInfo{id: info},
	}

	attempts := 0
	router := neighborWriterFunc(func(
		ctx context.Context,
		gotID string,
		gotAddress string,
	) error {
		attempts++

		if gotID != id.String() || gotAddress != info.UDPAddress {
			t.Fatalf("unexpected update: %q %q", gotID, gotAddress)
		}

		if _, ok := ctx.Deadline(); !ok {
			t.Fatal("router update has no deadline")
		}

		if attempts == 1 {
			return errors.New("router unavailable")
		}
		return nil
	})

	controller.syncRouter(context.Background(), router)

	if got, ok := controller.PeerInfo(id); !ok || got != info {
		t.Fatal("failed IPC update discarded learned metadata")
	}

	controller.syncRouter(context.Background(), router)

	if attempts != 2 {
		t.Fatalf("got %d attempts, want 2", attempts)
	}
}
