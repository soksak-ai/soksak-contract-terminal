package terminalcontract

import (
	"encoding/json"
	"fmt"
	"path/filepath"
	"regexp"
)

const ProtocolVersion uint32 = 1
const HandoffContract uint32 = 2

var unitName = regexp.MustCompile(`^[a-z0-9][a-z0-9-]*$`)

func DaemonBinaryPath(home string) string {
	return filepath.Join(home, "bin", fmt.Sprintf("soksak-ptyd-p%d", ProtocolVersion))
}

func ControlSocketPath(home string) string {
	return filepath.Join(home, "run", fmt.Sprintf("ptyd-p%d.sock", ProtocolVersion))
}

func StreamSocketPath(home string) string {
	return filepath.Join(home, "run", fmt.Sprintf("ptyd-p%d-stream.sock", ProtocolVersion))
}

func TokenPath(home string) string {
	return filepath.Join(home, "run", fmt.Sprintf("ptyd-p%d.token", ProtocolVersion))
}

func ServiceSocketPath(home string) string {
	return filepath.Join(home, "run", fmt.Sprintf("soksak-sidecar-terminal-p%d.sock", ProtocolVersion))
}

func SidecarBinaryPath(home, name string) (string, error) {
	if !unitName.MatchString(name) {
		return "", fmt.Errorf("invalid sidecar unit name %q", name)
	}
	unit := "soksak-sidecar-" + name
	return filepath.Join(home, "sidecars", unit, "dist", unit), nil
}

type Hello struct {
	Version   uint32  `json:"version"`
	Token     string  `json:"token"`
	ClientID  string  `json:"clientId"`
	Session   *uint64 `json:"session,omitempty"`
	FromSeq   *uint64 `json:"fromSeq,omitempty"`
	Subscribe bool    `json:"subscribe,omitempty"`
}

func NewHello(token, clientID string) Hello {
	return Hello{Version: ProtocolVersion, Token: token, ClientID: clientID}
}

type CreateOrAttachRequest struct {
	Op              string      `json:"op"`
	PaneID          string      `json:"paneId"`
	Cols            uint16      `json:"cols"`
	Rows            uint16      `json:"rows"`
	CWD             *string     `json:"cwd"`
	Shell           string      `json:"shell"`
	Environment     [][2]string `json:"env"`
	EnvironmentDrop []string    `json:"envRemove"`
	WindowLabel     string      `json:"windowLabel"`
	CheckpointKey   *string     `json:"checkpointPk,omitempty"`
	CheckpointKeyID *string     `json:"checkpointKeyId,omitempty"`
}

type SessionRequest struct {
	Op      string `json:"op"`
	Session uint64 `json:"session"`
}

type OperationRequest struct {
	Op string `json:"op"`
}

type WriteRequest struct {
	Op         string `json:"op"`
	Session    uint64 `json:"session"`
	DataBase64 string `json:"dataB64"`
}

type ResizeRequest struct {
	Op      string `json:"op"`
	Session uint64 `json:"session"`
	Cols    uint16 `json:"cols"`
	Rows    uint16 `json:"rows"`
}

type AckRequest struct {
	Op      string `json:"op"`
	Session uint64 `json:"session"`
	Bytes   uint64 `json:"bytes"`
}

type PaneRequest struct {
	Op     string `json:"op"`
	PaneID string `json:"paneId"`
}

type WindowRequest struct {
	Op          string `json:"op"`
	WindowLabel string `json:"windowLabel"`
}

type SessionInfo struct {
	Session     uint64  `json:"session"`
	PaneID      string  `json:"paneId"`
	ShellPID    uint32  `json:"shellPid"`
	Generation  uint64  `json:"generation"`
	WindowLabel *string `json:"windowLabel"`
}

type Reply struct {
	OK      bool            `json:"ok"`
	Code    string          `json:"code"`
	Message string          `json:"message"`
	Data    json.RawMessage `json:"data"`
}

func (reply Reply) DecodeData(target any) error {
	if !reply.OK {
		return fmt.Errorf("%s: %s", reply.Code, reply.Message)
	}
	if len(reply.Data) == 0 || string(reply.Data) == "null" {
		return nil
	}
	if target == nil {
		return nil
	}
	return json.Unmarshal(reply.Data, target)
}

type StreamAck struct {
	Session  uint64 `json:"session"`
	Mode     string `json:"mode"`
	StartSeq uint64 `json:"startSeq"`
}
