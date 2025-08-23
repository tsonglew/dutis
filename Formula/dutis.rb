class Dutis < Formula
  desc "macOS Application File Extension Manager"
  homepage "https://github.com/tsonglew/dutis"
  url "https://github.com/tsonglew/dutis/archive/refs/tags/v2.0.0.tar.gz"
  sha256 "49ca687604d210ae4ecf6a691ac5be3954238a167eaa12dd89f84c181f4a433b"
  license "MIT"
  head "https://github.com/tsonglew/dutis.git", branch: "main"

  depends_on "rust" => :build
  depends_on "duti"

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    # Test that the binary can run and show help
    system "#{bin}/dutis", "--help"
  end
end
