package ipc

import (
	"bytes"
	"context"
	"encoding/binary"
	"encoding/json"
	"fmt"
	"io"
	"log/slog"
	"net"
	"time"
)

const (
	maxFrame   = 64 * 1024
	ipcTimeout = 3 * time.Second
)

type Client struct {
	SocketPath string
}

type Status struct {
	UDPAddress string            `json:"udp_address"`
	Neighbors  map[string]string `json:"neighbors"`
}

// TODO: Makek Type an enum
type request struct {
	Version    int    `json:"version"`
	Type       string `json:"type"`
	PeerID     string `json:"peer_id,omitempty"`
	UDPAddress string `json:"udp_address,omitempty"`
}

type response struct {
	Ok    bool   `json:"ok"`
	Error string `json:"error,omitempty"`
	Status
}

func (c Client) GetStatus(ctx context.Context) (Status, error) {
	reply, err := c.call(ctx, request{Type: "get_status"})
	return reply.Status, err
}

func (c Client) UpsertNeighbor(ctx context.Context, peerID string, udpAddress string) error {
	_, err := c.call(ctx, request{
		Type:       "upsert_neighbor",
		PeerID:     peerID,
		UDPAddress: udpAddress,
	})
	return err
}

func (c Client) call(ctx context.Context, req request) (response, error) {
	var reply response

	ctx, cancel := context.WithTimeout(ctx, ipcTimeout)
	defer cancel()

	var dialer net.Dialer
	conn, err := dialer.DialContext(ctx, "unix", c.SocketPath)
	if err != nil {
		return reply, fmt.Errorf("connection to local router: %w", err)
	}
	defer conn.Close()

	// interrupt established connection if caller cancels
	stop := context.AfterFunc(ctx, func() {
		_ = conn.Close()
	})
	defer stop()

	deadline, _ := ctx.Deadline()
	if err := conn.SetDeadline(deadline); err != nil {
		return reply, err
	}

	// build request payload
	req.Version = 1
	body, err := json.Marshal(req)

	slog.Info("", "body", body)
	if err != nil {
		return reply, err
	}
	if len(body) > maxFrame {
		return reply, fmt.Errorf("request too large: %d bytes", len(body))
	}

	frame := make([]byte, 4+len(body))
	binary.BigEndian.PutUint32(frame[:4], uint32(len(body)))
	copy(frame[4:], body)

	// send payload and receive response
	if _, err := io.Copy(conn, bytes.NewReader(frame)); err != nil {
		return reply, err
	}

	var header [4]byte
	if _, err := io.ReadFull(conn, header[:]); err != nil {
		return reply, fmt.Errorf("reading response header: %w", err)
	}

	size := binary.BigEndian.Uint32(header[:])
	if size == 0 || size > maxFrame {
		return reply, fmt.Errorf("invalid response size: %d", size)
	}

	responseBody := make([]byte, size)
	if _, err := io.ReadFull(conn, responseBody); err != nil {
		return reply, fmt.Errorf("reading response body: %w", err)
	}

	// response too slow or abandoned so abandon return value
	if err := ctx.Err(); err != nil {
		return reply, err
	}

	if err := json.Unmarshal(responseBody, &reply); err != nil {
		return reply, fmt.Errorf("unmarshaling response: %w", err)
	}
	if !reply.Ok {
		return reply, fmt.Errorf("router rejected request: %s", reply.Error)
	}
	return reply, nil
}
