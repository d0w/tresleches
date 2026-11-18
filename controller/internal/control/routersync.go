package control

import (
	"context"
	"log/slog"
	"time"

	"github.com/libp2p/go-libp2p/core/peer"
)

const (
	routerResyncInterval = time.Second * 5
	routerUpdateTimeout  = time.Second * 3
)

type NeighborWriter interface {
	UpsertNeighbor(
		ctx context.Context,
		peerID string,
		udpAddress string,
	) error
}

func (n *NetworkingController) RunRouterSync(ctx context.Context, router NeighborWriter) {
	ticker := time.NewTicker(routerResyncInterval)
	defer ticker.Stop()

	for {
		if ctx.Err() != nil {
			return
		}
		n.syncRouter(ctx, router)

		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
		case <-n.routerWake:
		}
	}
}

func (n *NetworkingController) syncRouter(ctx context.Context, router NeighborWriter) {
	n.mutex.RLock()
	desired := make(map[peer.ID]PeerInfo, len(n.peerInfo))
	for id, info := range n.peerInfo {
		desired[id] = info
	}
	n.mutex.RUnlock()

	// send control signal to sync discovered neighbors
	for id, info := range desired {
		if ctx.Err() != nil {
			return
		}

		updateCtx, cancel := context.WithTimeout(ctx, routerUpdateTimeout)
		err := router.UpsertNeighbor(updateCtx, id.String(), info.UDPAddress)
		cancel()

		if ctx.Err() != nil {
			return
		}

		if err != nil {
			slog.Error("failed to update router", "error", err)
			continue
		}

		slog.Info("router synced neighbor", "peer_id", id.String(), "address", info.UDPAddress)

	}
}
