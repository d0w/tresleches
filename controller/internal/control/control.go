package control

import (
	"context"
	"fmt"
	"log/slog"
	"sync"
	"time"

	"github.com/libp2p/go-libp2p/core/host"
	"github.com/libp2p/go-libp2p/core/network"
	"github.com/libp2p/go-libp2p/core/peer"
)

const exchangeTimeout = 5 * time.Second

// NetworkingController handles control plane operations within the mesh network
// It will handle discovering remote peers' UDP endpoints to initiate data plane traffic
// by
type NetworkingController struct {
	host       host.Host
	mutex      sync.RWMutex
	local      PeerInfo
	peerInfo   map[peer.ID]PeerInfo
	routerWake chan struct{}
}

func NewNetworkingController(h host.Host, local PeerInfo) (*NetworkingController, error) {
	if err := validatePeerInfo(local); err != nil {
		return nil, fmt.Errorf("invalid local peer info: %w", err)
	}

	return &NetworkingController{
		host:       h,
		local:      local,
		peerInfo:   make(map[peer.ID]PeerInfo),
		routerWake: make(chan struct{}, 1),
	}, nil
}

func (n *NetworkingController) PeerInfo(id peer.ID) (PeerInfo, bool) {
	n.mutex.RLock()
	defer n.mutex.RUnlock()
	info, found := n.peerInfo[id]

	return info, found
}

// Register effectively adds the local peer to the control network
// by adding a stream handler for incoming traffic
func (n *NetworkingController) Register() {
	n.host.SetStreamHandler(ProtocolID, n.handleStream)
}

func (n *NetworkingController) Unregister() {
	n.host.SetStreamHandler(ProtocolID, nil)
}

// Exchange initiates a network exchange with a remote peer
func (n *NetworkingController) Exchange(ctx context.Context, remotePeer peer.ID) (err error) {
	ctx, cancel := context.WithTimeout(ctx, exchangeTimeout)
	defer cancel()

	stream, err := n.host.NewStream(ctx, remotePeer, ProtocolID)
	if err != nil {
		return fmt.Errorf("failed to open stream: %w", err)
	}

	// cleanup
	defer func() {
		if err != nil {
			_ = stream.Reset()
		} else {
			_ = stream.Close()
		}
	}()

	// close stream/interrupt IO if context is cancelled
	stop := context.AfterFunc(ctx, func() {
		_ = stream.Reset()
	})
	defer stop()

	deadline, _ := ctx.Deadline()
	if err := stream.SetDeadline(deadline); err != nil {
		return fmt.Errorf("failed to set deadline: %w", err)
	}

	// initator writes first then reads
	if err := writePeerInfo(stream, n.local); err != nil {
		return fmt.Errorf("failed to write local peer info: %w", err)
	}

	info, err := readPeerInfo(stream)
	if err != nil {
		return fmt.Errorf("failed to read remote peer info: %w", err)
	}

	if err := ctx.Err(); err != nil {
		return err
	}

	n.remember(stream.Conn().RemotePeer(), info)

	return nil
}

func (n *NetworkingController) handleStream(stream network.Stream) {
	// slog.Debug("incoming peer exchange")
	if err := n.respond(stream); err != nil {
		_ = stream.Reset()
		slog.Error("incoming peer exchange failed", "peer", stream.Conn().RemotePeer(), "error", err)
	}

	if err := stream.Close(); err != nil {
		slog.Error("failed to close stream", "error", err)
	}
}

func (n *NetworkingController) respond(stream network.Stream) error {
	if err := stream.SetDeadline(time.Now().Add(exchangeTimeout)); err != nil {
		return err
	}

	// receiver reads first, then writes
	info, err := readPeerInfo(stream)
	if err != nil {
		return fmt.Errorf("failed to read peer info: %w", err)
	}

	if err := writePeerInfo(stream, n.local); err != nil {
		return fmt.Errorf("failed to write peer info: %w", err)
	}

	n.remember(stream.Conn().RemotePeer(), info)
	return nil
}

func (n *NetworkingController) remember(id peer.ID, info PeerInfo) {
	n.mutex.Lock()
	n.peerInfo[id] = info
	n.mutex.Unlock()

	slog.Info("peer info learned", "peer", id, "udp_address", info.UDPAddress)

	select {
	case n.routerWake <- struct{}{}:
	default:
	}
}
