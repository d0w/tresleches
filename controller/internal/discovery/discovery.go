package discovery

import (
	"context"
	"log/slog"
	"time"

	"github.com/d0w/tresleches/discovery/constants"
	"github.com/libp2p/go-libp2p/core/discovery"
	"github.com/libp2p/go-libp2p/core/host"
	"github.com/libp2p/go-libp2p/core/peer"
	"github.com/libp2p/go-libp2p/core/routing"
	routingDiscovery "github.com/libp2p/go-libp2p/p2p/discovery/routing"
	discoveryUtil "github.com/libp2p/go-libp2p/p2p/discovery/util"
)

//
// type DiscoveryService interface {
// 	Discover(ctx context.Context, h host.Host, router routing.ContentRouting)
// }

// Discover starts advertising on the namespace and synchronously
// polls to look for peers and adds them to the DHT
func Discover(
	ctx context.Context,
	h host.Host,
	router routing.ContentRouting,
	onPeer func(context.Context, peer.ID) error,
) {
	rd := routingDiscovery.NewRoutingDiscovery(router)

	// advertise service
	discoveryUtil.Advertise(ctx, rd, constants.Namespace)

	ticker := time.NewTicker(constants.AdvertiseInterval)
	defer ticker.Stop()

	slog.Info("Searching...")
	for {
		searchCtx, cancelSearch := context.WithTimeout(ctx, time.Second*20)
		defer cancelSearch()

		candidates, err := rd.FindPeers(searchCtx, constants.Namespace, discovery.Limit(20))
		if err != nil {
			slog.Warn("peer search failed", "err", err)
		} else {
			for candidate := range candidates {
				if searchCtx.Err() != nil {
					slog.Error("error: ", "err", searchCtx.Err())
					break
				}

				if candidate.ID == h.ID() {
					continue
				}
				slog.Debug("Found peer", "peer", candidate)
				// if h.Network().Connectedness(candidate.ID) == network.Connected {
				// 	continue
				// }

				dialCtx, cancelDial := context.WithTimeout(searchCtx, 5*time.Second)
				err := h.Connect(dialCtx, candidate)
				cancelDial()

				if err != nil {
					slog.Error("failed to connect to peer", "id", candidate.ID, "err", err)
					continue
				}

				if err := onPeer(searchCtx, candidate.ID); err != nil {
					slog.Error("peer info exchange failed", "peer", candidate.ID, "err", err)
				}

				slog.Info("connected to peer", "id", candidate.ID)
			}
		}

		cancelSearch()

		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
		}
	}
}
