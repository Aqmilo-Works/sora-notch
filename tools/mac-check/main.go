// A stand-in C compiler and archiver for `cargo check --target *-apple-darwin` on Windows: build scripts run, but no
// C code is needed to type-check Rust, so each output file is written empty (objects) or as an empty archive.
package main

import (
	"os"
	"path/filepath"
	"strings"
)

func main() {
	args := os.Args[1:]
	name := strings.ToLower(filepath.Base(os.Args[0]))
	if strings.Contains(name, "ar") && !strings.Contains(name, "cc") {
		for _, a := range args {
			if strings.HasSuffix(a, ".a") {
				os.WriteFile(a, []byte("!<arch>\n"), 0644)
			}
		}
		return
	}
	for i, a := range args {
		if a == "-o" && i+1 < len(args) {
			os.WriteFile(args[i+1], nil, 0644)
		} else if strings.HasPrefix(a, "-o") && len(a) > 2 {
			os.WriteFile(a[2:], nil, 0644)
		}
		if a == "--version" || a == "-v" {
			os.Stdout.WriteString("clang version 17.0.0 (fake)\n")
		}
	}
}
