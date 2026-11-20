package tun

import (
	"context"
	"fmt"
	"log/slog"
	"os"
)

const bufferSize = 65536 // ip packet max

func Observe(ctx context.Context, file *os.File) error {
	stopClose := context.AfterFunc(ctx, func() {
		_ = file.Close()
	})
	defer stopClose()

	buffer := make([]byte, 65536)

	for {
		if ctx.Err() != nil {
			return nil
		}

		n, err := file.Read(buffer)
		if err != nil {
			if ctx.Err() != nil {
				return nil
			}
			return fmt.Errorf("read TUN: %w", err)
		}

		if n == 0 {
			continue
		}

		slog.Info("TUN Packet", "interface", file.Name(), "bytes", n, "ip_version", buffer[0]>>4)
	}
}
