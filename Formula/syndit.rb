class Syndit < Formula
  desc "CLI and MCP runtime for the syndit agent protocol"
  homepage "https://github.com/intuitive-compute/syndit"
  url "https://github.com/intuitive-compute/syndit/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "PLACEHOLDER"
  license "MIT OR Apache-2.0"
  head "https://github.com/intuitive-compute/syndit.git", branch: "main"

  depends_on "rust" => :build
  depends_on "protobuf" => :build

  def install
    system "cargo", "install", "--locked", "--root", prefix,
           "--path", "crates/syndit-cli"
    system "cargo", "install", "--locked", "--root", prefix,
           "--path", "crates/agent-runtime"
  end

  test do
    assert_match "syndit", shell_output("#{bin}/syndit --version")
  end
end
