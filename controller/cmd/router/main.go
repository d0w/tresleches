package main

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"log/slog"
	"os"

	"github.com/d0w/tresleches/discovery/internal/ipc"
)

// Test main function to probe router control socket

func main() {
	if err := run(); err != nil {
		slog.Error(err.Error())
		os.Exit(1)
	}
}

func run() error {
	socket := flag.String("socket", "", "Local Rust control socket")
	flag.Parse()

	client := &ipc.Client{SocketPath: *socket}
	ctx := context.Background()
	args := flag.Args()

	switch {
	case len(args) == 1 && args[0] == "get-status":
		status, err := client.GetStatus(ctx)
		if err != nil {
			return err
		}
		return json.NewEncoder(os.Stdout).Encode(status)

	case len(args) == 3 && args[0] == "upsert-neighbor":
		if err := client.UpsertNeighbor(ctx, args[1], args[2]); err != nil {
			return err
		}
		slog.Info("Neighbor configured")
		return nil
	default:
		return fmt.Errorf(
			"usage: routerctl -socket PATH get-status | upsert-neighbor PEER_ID UDP_ADDRESS",
		)
	}
}
