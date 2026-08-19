package terminalcontract

import (
	"encoding/json"
	"path/filepath"
	"testing"
)

func TestProtocolPathsAreVersionedUnderTheDeclaredHome(t *testing.T) {
	home := "/installation"
	if got := DaemonBinaryPath(home); got != filepath.Join(home, "bin", "soksak-ptyd-p1") {
		t.Fatalf("daemon binary path = %q", got)
	}
	if got := ControlSocketPath(home); got != filepath.Join(home, "run", "ptyd-p1.sock") {
		t.Fatalf("control socket path = %q", got)
	}
	if got := StreamSocketPath(home); got != filepath.Join(home, "run", "ptyd-p1-stream.sock") {
		t.Fatalf("stream socket path = %q", got)
	}
	if got := TokenPath(home); got != filepath.Join(home, "run", "ptyd-p1.token") {
		t.Fatalf("token path = %q", got)
	}
}

func TestCreateOrAttachWireUsesTheRustContractFieldNames(t *testing.T) {
	request := CreateOrAttachRequest{
		Op:          "createOrAttach",
		PaneID:      "tab-a",
		Cols:        80,
		Rows:        24,
		Shell:       "/bin/zsh",
		Environment: [][2]string{{"TERM", "xterm-256color"}},
		WindowLabel: "win-a",
	}
	encoded, err := json.Marshal(request)
	if err != nil {
		t.Fatal(err)
	}
	var fields map[string]any
	if err := json.Unmarshal(encoded, &fields); err != nil {
		t.Fatal(err)
	}
	for _, name := range []string{"op", "paneId", "cols", "rows", "shell", "env", "envRemove", "windowLabel"} {
		if _, found := fields[name]; !found {
			t.Errorf("wire field %q is missing from %s", name, encoded)
		}
	}
}

func TestHelloCarriesTheCurrentProtocolAndClientIdentity(t *testing.T) {
	hello := NewHello("token", "wails")
	if hello.Version != ProtocolVersion || hello.Token != "token" || hello.ClientID != "wails" {
		t.Fatalf("hello = %#v", hello)
	}
}

func TestReplyCanValidateSuccessWithoutDecodingData(t *testing.T) {
	reply := Reply{OK: true, Code: "OK", Data: json.RawMessage(`{"pid":1}`)}
	if err := reply.DecodeData(nil); err != nil {
		t.Fatal(err)
	}
}
