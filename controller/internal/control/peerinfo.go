package control

import (
	"bytes"
	"encoding/binary"
	"encoding/json"
	"fmt"
	"io"
	"net/netip"

	"github.com/libp2p/go-libp2p/core/protocol"
)

const (
	ProtocolID      protocol.ID = "/tresleches/control/1.0.0/"
	maxPeerInfoSize             = 4096
)

type PeerInfo struct {
	Version    int    `json:"version"`
	UDPAddress string `json:"udp_address"`
}

func validatePeerInfo(info PeerInfo) error {
	if info.Version != 1 {
		return fmt.Errorf("invalid peeer version")
	}

	endpoint, err := netip.ParseAddrPort(info.UDPAddress)
	if err != nil {
		return fmt.Errorf("invalid address string")
	}

	addr := endpoint.Addr().Unmap()
	if endpoint.Port() == 0 || addr.IsUnspecified() || addr.IsMulticast() {
		return fmt.Errorf("unstable UDP endpoint: %s", info.UDPAddress)
	}

	return nil
}

func writePeerInfo(w io.Writer, info PeerInfo) error {
	body, err := json.Marshal(info)
	if err != nil {
		return err
	}

	if len(body) > maxPeerInfoSize {
		return fmt.Errorf("peer info exceeds max size: %d", maxPeerInfoSize)
	}

	// length prefixed frame. length(4B) | body( len(body)B )
	frame := make([]byte, 4+len(body))
	binary.BigEndian.PutUint32(frame, uint32(len(body)))
	copy(frame[4:], body)

	_, err = io.Copy(w, bytes.NewReader(frame))

	return err
}

func readPeerInfo(r io.Reader) (PeerInfo, error) {
	var info PeerInfo
	var header [4]byte

	if _, err := io.ReadFull(r, header[:]); err != nil {
		return info, err
	}

	size := binary.BigEndian.Uint32(header[:])
	if size == 0 || size > maxPeerInfoSize {
		return info, fmt.Errorf("invalid peer info size: %d", size)
	}

	body := make([]byte, int(size))

	if _, err := io.ReadFull(r, body); err != nil {
		return info, fmt.Errorf("failed to read peer info body: %w", err)
	}

	if err := json.Unmarshal(body, &info); err != nil {
		return info, fmt.Errorf("failed to unmarshal peer info: %w", err)
	}

	if err := validatePeerInfo(info); err != nil {
		return info, fmt.Errorf("invalid peer info: %w", err)
	}

	return info, nil
}
