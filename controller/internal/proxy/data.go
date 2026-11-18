package proxy

import "fmt"

// DataServer is the dataplane server that will send/receive data
type DataServer struct {
	Address   string `json:"address"`
	Port      int    `json:"port"`
	connected bool
}

type Option func(*DataServer)

type DataServerConfig struct {
	Address string `json:"address"`
	Port    int    `json:"port"`
}

type DataProxy interface {
	SendData(data []byte, destination string) error
	GetAddress() (string, error)
	Connect(dataServer *DataServerConfig, opts ...Option) error
	Disconnect() error
}

func NewDataServer(address string, port int) *DataServer {
	return &DataServer{
		Address: address,
		Port:    port,
	}
}

func (d *DataServer) GetAddress() (string, error) {
	if !d.connected {
		return "", fmt.Errorf("not connected to data server")
	}
	return d.Address, nil
}
