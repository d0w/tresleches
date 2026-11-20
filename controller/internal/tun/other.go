//go:build !linux

package tun

import (
	"fmt"
	"os"
)

func Open(name string) (*os.File, error) {
	return nil, fmt.Errorf("TUN interface %q requires Linux", name)
}
