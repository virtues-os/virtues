#!/bin/bash
#
# Virtues Mac Installer
# Interactive installer with GUI dialogs for macOS
#
# Usage:
#   curl -sSL https://github.com/virtues-os/virtues/releases/latest/download/installer.sh | bash
#
# With parameters:
#   curl -sSL ... | bash -s -- --token YOUR_TOKEN --endpoint https://your-server.com
#

set -e

# Colors for terminal output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
INSTALL_DIR="/usr/local/bin"
CONFIG_DIR="$HOME/.virtues"
BINARY_NAME="virtues-mac"
REPO="virtues-os/virtues"

# Parameters (can be passed via command line or prompted)
TOKEN=""
ENDPOINT=""
INTERACTIVE=true

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --token)
            TOKEN="$2"
            shift 2
            ;;
        --endpoint)
            ENDPOINT="$2"
            shift 2
            ;;
        --non-interactive)
            INTERACTIVE=false
            shift
            ;;
        --help)
            echo "Virtues Mac Installer"
            echo ""
            echo "Usage: installer.sh [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --token TOKEN       Device pairing token from web UI"
            echo "  --endpoint URL      Server endpoint (e.g., https://virtues.example.com)"
            echo "  --non-interactive   Skip GUI dialogs, use command line only"
            echo "  --help              Show this help message"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Helper functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if running on macOS
check_macos() {
    if [[ "$(uname)" != "Darwin" ]]; then
        log_error "This installer is for macOS only"
        exit 1
    fi
}

# Show GUI dialog (falls back to terminal if not available)
show_dialog() {
    local message="$1"
    local title="${2:-Virtues Installer}"

    if $INTERACTIVE && command -v osascript &> /dev/null; then
        osascript -e "display dialog \"$message\" with title \"$title\" buttons {\"OK\"} default button \"OK\"" 2>/dev/null || true
    fi
}

# Show GUI prompt for input
prompt_dialog() {
    local message="$1"
    local default_value="$2"
    local title="${3:-Virtues Installer}"

    if $INTERACTIVE && command -v osascript &> /dev/null; then
        result=$(osascript <<EOF
display dialog "$message" default answer "$default_value" with title "$title" buttons {"Cancel", "OK"} default button "OK"
text returned of result
EOF
) || return 1
        echo "$result"
    else
        read -p "$message [$default_value]: " result
        echo "${result:-$default_value}"
    fi
}

# Show yes/no dialog
confirm_dialog() {
    local message="$1"
    local title="${2:-Virtues Installer}"

    if $INTERACTIVE && command -v osascript &> /dev/null; then
        result=$(osascript <<EOF
display dialog "$message" with title "$title" buttons {"No", "Yes"} default button "Yes"
button returned of result
EOF
) 2>/dev/null
        [[ "$result" == "Yes" ]]
    else
        read -p "$message [y/N]: " result
        [[ "$result" =~ ^[Yy]$ ]]
    fi
}

# Detect architecture
detect_arch() {
    ARCH=$(uname -m)
    log_info "Detected architecture: $ARCH"
}

# Download and install binary
install_binary() {
    log_info "Downloading Virtues Mac..."

    local download_url="https://github.com/$REPO/releases/latest/download/virtues-mac-universal.tar.gz"
    local temp_dir=$(mktemp -d)

    # Download
    if ! curl -L -o "$temp_dir/virtues-mac.tar.gz" "$download_url" 2>/dev/null; then
        log_error "Failed to download from $download_url"
        rm -rf "$temp_dir"
        exit 1
    fi

    # Extract
    log_info "Extracting..."
    tar -xzf "$temp_dir/virtues-mac.tar.gz" -C "$temp_dir"

    # Install (requires sudo)
    log_info "Installing to $INSTALL_DIR (requires admin password)..."
    sudo mv "$temp_dir/$BINARY_NAME" "$INSTALL_DIR/$BINARY_NAME"
    sudo chmod +x "$INSTALL_DIR/$BINARY_NAME"

    # Clean up
    rm -rf "$temp_dir"

    log_success "Binary installed to $INSTALL_DIR/$BINARY_NAME"
}

# Check if already installed
check_existing() {
    if command -v $BINARY_NAME &> /dev/null; then
        local current_version=$($BINARY_NAME --version 2>/dev/null || echo "unknown")
        log_warn "Virtues Mac is already installed (version: $current_version)"

        if confirm_dialog "Virtues Mac is already installed. Do you want to reinstall/upgrade?"; then
            return 0
        else
            log_info "Installation cancelled"
            exit 0
        fi
    fi
}

# Initialize with token and endpoint
initialize() {
    if [[ -n "$TOKEN" && -n "$ENDPOINT" ]]; then
        log_info "Initializing with provided token..."
        $BINARY_NAME init "$TOKEN" --endpoint "$ENDPOINT"
        log_success "Initialized successfully"
    elif $INTERACTIVE; then
        if confirm_dialog "Would you like to configure Virtues Mac now?\n\nYou'll need a pairing token from your Virtues web dashboard."; then
            # Prompt for endpoint
            ENDPOINT=$(prompt_dialog "Enter your Virtues server endpoint:" "https://virtues.example.com" "Server Endpoint")
            if [[ -z "$ENDPOINT" ]]; then
                log_warn "Skipping configuration - no endpoint provided"
                return
            fi

            # Prompt for token
            TOKEN=$(prompt_dialog "Enter your device pairing token:" "" "Pairing Token")
            if [[ -z "$TOKEN" ]]; then
                log_warn "Skipping configuration - no token provided"
                return
            fi

            # Initialize
            $BINARY_NAME init "$TOKEN" --endpoint "$ENDPOINT"
            log_success "Initialized successfully"
        else
            log_info "Skipping configuration. You can run '$BINARY_NAME init <token>' later."
        fi
    else
        log_info "Run '$BINARY_NAME init <token> --endpoint <url>' to configure"
    fi
}

# Start daemon
start_daemon() {
    if $INTERACTIVE; then
        if confirm_dialog "Would you like to start Virtues Mac now?\n\nIt will run in the background and start automatically on login."; then
            log_info "Starting daemon..."
            $BINARY_NAME daemon
            log_success "Daemon installed and started"
        else
            log_info "You can start later with: $BINARY_NAME daemon"
        fi
    else
        log_info "Run '$BINARY_NAME daemon' to start the background service"
    fi
}

# Main installation flow
main() {
    echo ""
    echo "╔══════════════════════════════════════╗"
    echo "║       Virtues Mac Installer          ║"
    echo "╚══════════════════════════════════════╝"
    echo ""

    check_macos
    detect_arch
    check_existing
    install_binary

    # Only initialize if token/endpoint provided or interactive
    if [[ -n "$TOKEN" ]] || $INTERACTIVE; then
        initialize
    fi

    # Offer to start daemon
    if [[ -f "$CONFIG_DIR/config.json" ]]; then
        start_daemon
    fi

    echo ""
    log_success "Installation complete!"
    echo ""
    echo "Next steps:"
    echo "  1. Get a pairing token from your Virtues web dashboard"
    echo "  2. Run: $BINARY_NAME init <token> --endpoint <url>"
    echo "  3. Run: $BINARY_NAME daemon"
    echo ""
    echo "For help: $BINARY_NAME --help"
    echo ""

    show_dialog "Virtues Mac has been installed successfully!\n\nRun 'virtues-mac --help' in Terminal for usage instructions." "Installation Complete"
}

main
