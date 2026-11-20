//go:build linux

package tun

import (
	"fmt"
	"os"
	"strings"

	"golang.org/x/sys/unix"
)

func Open(name string) (*os.File, error) {
	if name == "" || strings.ContainsAny(name, "\x00%") {
		return nil, fmt.Errorf("TUN interface name: %w", name)
	}

	request, err := unix.NewIfreq(name)
	if err != nil {
		return nil, fmt.Errorf("Error creating interface request: %w", err)
	}

	// no ip packets | omit extra tun header | refuse attaching to existing interface
	request.SetUint16(unix.IFF_TUN | unix.IFF_NO_PI | unix.IFF_TUN_EXCL)

	// read/write | dont block os thread | prevent leak descriptor to child processes
	fd, err := unix.Open("/dev/net/tun", unix.O_RDWR|unix.O_NONBLOCK|unix.O_CLOEXEC, 0)
	if err != nil {
		return nil, fmt.Errorf("Opening tun interface: %w", err)
	}

	if err := unix.IoctlIfreq(fd, unix.TUNSETIFF, request); err != nil {
		_ = unix.Close(fd)
		return nil, fmt.Errorf("create tun %q: %w", name, err)
	}

	return os.NewFile(uintptr(fd), name), nil
}
