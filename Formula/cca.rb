# Homebrew Formula for CCA (Claude Code Agent)
#
# Installation:
#   brew tap your-org/cca https://github.com/your-org/cca
#   brew install cca
#
# Or directly:
#   brew install your-org/cca/cca

class Cca < Formula
  desc "Claude Code Agent - AI-powered multi-agent orchestration system"
  homepage "https://github.com/your-org/cca"
  version "0.3.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/your-org/cca/releases/download/v#{version}/cca-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_SHA256_MAC_ARM64"
    end
    on_intel do
      url "https://github.com/your-org/cca/releases/download/v#{version}/cca-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_SHA256_MAC_X64"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/your-org/cca/releases/download/v#{version}/cca-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "PLACEHOLDER_SHA256_LINUX_ARM64"
    end
    on_intel do
      url "https://github.com/your-org/cca/releases/download/v#{version}/cca-v#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "PLACEHOLDER_SHA256_LINUX_X64"
    end
  end

  depends_on "postgresql@16" => :optional
  depends_on "redis" => :optional

  def install
    bin.install "ccad"

    # Install documentation if present
    doc.install "README.md" if File.exist?("README.md")
    doc.install Dir["docs/*"] if Dir.exist?("docs")
  end

  def caveats
    <<~EOS
      CCA daemon (ccad) has been installed.

      Quick Start:
        ccad --help

      Configuration:
        CCA uses environment variables or a config file for configuration.
        Default config location: ~/.config/cca/config.toml

      Required Services:
        CCA requires Redis and PostgreSQL (with pgvector extension).

        Option 1 - Using Homebrew services:
          brew install redis postgresql@16
          brew services start redis
          brew services start postgresql@16

          # Install pgvector extension
          psql -c "CREATE EXTENSION IF NOT EXISTS vector;"

        Option 2 - Using Docker (recommended):
          Download docker-compose.yml from the CCA repository and run:
          docker compose up -d

      Environment Variables:
        CCA__POSTGRES__URL=postgres://user:pass@localhost:5432/cca
        CCA__REDIS__URL=redis://localhost:6379
        CCA__DAEMON__BIND_ADDRESS=127.0.0.1:9200

      For more information:
        https://github.com/your-org/cca
    EOS
  end

  service do
    run [opt_bin/"ccad"]
    keep_alive true
    working_dir var/"cca"
    log_path var/"log/cca.log"
    error_log_path var/"log/cca-error.log"
    environment_variables CCA__DAEMON__BIND_ADDRESS: "127.0.0.1:9200"
  end

  test do
    # Test that the binary runs and shows version
    assert_match version.to_s, shell_output("#{bin}/ccad --version 2>&1", 0)

    # Test that help works
    output = shell_output("#{bin}/ccad --help 2>&1", 0)
    assert_match "cca", output.downcase
  end
end
