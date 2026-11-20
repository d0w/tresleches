package main

import (
	"context"
	"flag"
	"fmt"
	"log/slog"
	"net/netip"
	"os"
	"os/signal"
	"strings"
	"sync"
	"syscall"
	"time"

	"github.com/d0w/tresleches/discovery/internal/control"
	"github.com/d0w/tresleches/discovery/internal/discovery"
	"github.com/d0w/tresleches/discovery/internal/ipc"
	"github.com/d0w/tresleches/discovery/internal/tun"
	"github.com/libp2p/go-libp2p"
	dht "github.com/libp2p/go-libp2p-kad-dht"
	"github.com/libp2p/go-libp2p/core/network"
	"github.com/libp2p/go-libp2p/core/peer"
)

type Config struct {
	Listen         string
	Bootstrap      string
	Seeds          string
	RouterEndpoint string
	RouterSocket   string
	TUNName        string
}

func parseFlags() Config {
	var cfg Config

	flag.StringVar(
		&cfg.Listen, "listen", "/ip4/127.0.0.1/tcp/0", "Discovery listen multiaddress",
	)
	flag.StringVar(&cfg.Bootstrap, "bootstrap", "", "Bootstrap multiaddress")
	flag.StringVar(&cfg.Seeds, "seeds", "", "Comma separated peer IDs to resolve")
	// logLevel := flag.String("log-level", "info", "Log level")
	flag.StringVar(
		&cfg.RouterEndpoint, "udp-advertise", "", "Advertised UDP endpoint for router",
	)
	flag.StringVar(&cfg.RouterSocket, "router-socket", "", "Local Rust router Unix socket (optional)")
	flag.StringVar(&cfg.TUNName, "tun-name", "", "TUN interface name")

	flag.Parse()

	return cfg
}

func main() {
	// level := slog.LevelInfo
	// if *logLevel == "debug" {
	// 	level = slog.LevelDebug
	// }
	cfg := parseFlags()

	if err := run(cfg); err != nil {
		slog.Error("controller failed", "err", err)
		os.Exit(1)
	}
}

func run(cfg Config) error {
	// finding targets
	var targets []peer.ID

	if strings.TrimSpace(cfg.Seeds) != "" {
		rawTargets := strings.Split(cfg.Seeds, ",")

		for _, raw := range rawTargets {
			id, err := peer.Decode(strings.TrimSpace(raw))
			if err != nil {
				return fmt.Errorf("parse target %q: %w", raw, err)
			}

			targets = append(targets, id)
		}
	}

	// finding seed peers to seed dht with
	var seeds []peer.AddrInfo

	if cfg.Bootstrap != "" {
		info, err := peer.AddrInfoFromString(cfg.Bootstrap)
		if err != nil {
			return fmt.Errorf("parse bootstrap address: %w", err)
		}
		seeds = append(seeds, *info)
	}

	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()

	// create host
	h, err := libp2p.New(libp2p.ListenAddrStrings(cfg.Listen))
	if err != nil {
		return fmt.Errorf("%w", err)
	}
	defer h.Close()

	// setting logger
	prefix := fmt.Sprintf("[%s]", h.ID()[0:8])
	logOpts := &slog.HandlerOptions{
		ReplaceAttr: func(groups []string, a slog.Attr) slog.Attr {
			if len(groups) == 0 && a.Key == slog.MessageKey {
				a.Value = slog.StringValue(prefix + a.Value.String())
			}
			return a
		},
		Level: slog.LevelDebug,
	}
	logHandler := slog.NewTextHandler(os.Stdout, logOpts)
	slog.SetDefault(slog.New(logHandler))

	// Setup TUN interface
	var tunFile *os.File

	if cfg.TUNName != "" {
		tunFile, err = tun.Open(cfg.TUNName)
		if err != nil {
			return err
		}
		defer tunFile.Close()

		slog.Info("TUN interface configured", "interface", tunFile.Name())
	}

	// Stale: Check the local router separately from its advertised UDP endpoint.
	var router *ipc.Client

	if cfg.RouterSocket != "" {
		advertised, err := netip.ParseAddrPort(cfg.RouterEndpoint)
		if err != nil {
			return fmt.Errorf("error parsing router endpoint: %w", err)
		}

		router = &ipc.Client{SocketPath: cfg.RouterSocket}
		status, err := router.GetStatus(ctx)
		if err != nil {
			return fmt.Errorf("error getting router status: %w", err)
		}

		bound, err := netip.ParseAddrPort(status.UDPAddress)
		if err != nil {
			return fmt.Errorf("error parsing router udp address: %w", err)
		}

		if bound.Addr().Unmap() != advertised.Addr().Unmap() || bound.Port() != advertised.Port() {
			return fmt.Errorf("advertised udp address does not match router's bound address")
		}

	}

	// Control plane setup
	controller, err := control.NewNetworkingController(
		h,
		control.PeerInfo{
			Version:    1,
			UDPAddress: cfg.RouterEndpoint,
		},
	)
	if err != nil {
		return fmt.Errorf("%w", err)
	}

	controller.Register()

	// hook to monitor connections
	h.Network().Notify(&network.NotifyBundle{
		ConnectedF: func(_ network.Network, conn network.Conn) {
			slog.Debug("connection opened",
				"peer", conn.RemotePeer().String(),
				"address", conn.RemoteMultiaddr().String(),
				"direction", conn.Stat().Direction,
			)
		},
	})

	// dht initialization
	idht, err := dht.New(h, dht.Mode(dht.ModeServer), dht.ProtocolPrefix("/tresleches"), dht.BootstrapPeers(seeds...))
	dht.BootstrapPeers(seeds...)

	if err != nil {
		return fmt.Errorf("%w", err)
	}

	defer idht.Close()

	slog.Info(fmt.Sprintf("peer ID: %s", h.ID()))

	// print node info
	addresses, err := peer.AddrInfoToP2pAddrs(&peer.AddrInfo{
		ID:    h.ID(),
		Addrs: h.Addrs(),
	})
	if err != nil {
		return fmt.Errorf("%w", err)
	}

	for _, address := range addresses {
		slog.Info(fmt.Sprintf("listen: %s", address))
	}

	// dial seeds
	for _, seed := range seeds {
		dialCtx, cancel := context.WithTimeout(ctx, 10*time.Second)
		err := h.Connect(dialCtx, seed)
		cancel()

		if err != nil {
			return fmt.Errorf("connect bootstrap %s: %w", seed.ID, err)
		}
	}

	if err := idht.Bootstrap(ctx); err != nil {
		return fmt.Errorf("%w", err)
	}

	var workers sync.WaitGroup

	workers.Add(1)
	go func() {
		defer workers.Done()
		discovery.Discover(ctx, h, idht, controller.Exchange)
	}()

	if router != nil {
		workers.Add(1)
		go func() {
			defer workers.Done()
			controller.RunRouterSync(ctx, router)
		}()
	}

	tunErrors := make(chan error, 1)
	if tunFile != nil {
		workers.Add(1)
		go func() {
			defer workers.Done()
			if err := tun.Observe(ctx, tunFile); err != nil {
				tunErrors <- err
				stop()
			}
		}()
	}

	<-ctx.Done()

	slog.Info("shutting down...")
	workers.Wait()

	select {
	case err := <-tunErrors:
		return err
	default:
		return nil
	}
}

// func findEventually(
// 	ctx context.Context,
// 	idht *dht.IpfsDHT,
// 	target peer.ID,
// ) (peer.AddrInfo, error) {
// 	ticker := time.NewTicker(500 * time.Millisecond)
// 	defer ticker.Stop()
//
// 	for {
// 		attemptCtx, cancel := context.WithTimeout(ctx, 3*time.Second)
// 		info, err := idht.FindPeer(attemptCtx, target)
// 		cancel()
//
// 		if err == nil && len(info.Addrs) > 0 {
// 			return info, nil
// 		}
//
// 		select {
// 		case <-ctx.Done():
// 			return peer.AddrInfo{}, fmt.Errorf(
// 				"resolve %s: %w",
// 				target,
// 				ctx.Err(),
// 			)
//
// 		case <-ticker.C:
// 		}
// 	}
// }
