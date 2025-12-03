#!/bin/bash
#
# Virtues Mac Installer
# Downloads and installs Virtues.app from DMG
#
# Usage:
#   curl -sSL https://github.com/virtues-os/virtues/releases/latest/download/installer.sh | bash
#

set -e

# Colors for terminal output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
APP_NAME="Virtues.app"
INSTALL_DIR="/Applications"
REPO="virtues-os/virtues"

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

# Check if already installed
check_existing() {
    if [[ -d "$INSTALL_DIR/$APP_NAME" ]]; then
        log_warn "Virtues is already installed at $INSTALL_DIR/$APP_NAME"
        read -p "Do you want to replace it? [y/N]: " result
        if [[ ! "$result" =~ ^[Yy]$ ]]; then
            log_info "Installation cancelled"
            exit 0
        fi
        log_info "Removing existing installation..."
        rm -rf "$INSTALL_DIR/$APP_NAME"
    fi
}

# Download and install from DMG
install_app() {
    log_info "Downloading Virtues for Mac..."

    local download_url="https://github.com/$REPO/releases/download/mac-latest/Virtues.dmg"
    local temp_dir=$(mktemp -d)
    local dmg_path="$temp_dir/Virtues.dmg"
    local mount_point="$temp_dir/Virtues"

    # Download DMG
    if ! curl -L -o "$dmg_path" "$download_url" 2>/dev/null; then
        log_error "Failed to download from $download_url"
        rm -rf "$temp_dir"
        exit 1
    fi

    log_info "Mounting DMG..."

    # Mount the DMG
    hdiutil attach "$dmg_path" -mountpoint "$mount_point" -nobrowse -quiet

    # Copy app to Applications
    log_info "Installing to $INSTALL_DIR..."
    cp -R "$mount_point/Virtues.app" "$INSTALL_DIR/"

    # Unmount DMG
    log_info "Cleaning up..."
    hdiutil detach "$mount_point" -quiet

    # Clean up temp files
    rm -rf "$temp_dir"

    log_success "Virtues installed to $INSTALL_DIR/$APP_NAME"
}

# Remove quarantine attribute (if present)
remove_quarantine() {
    if xattr -l "$INSTALL_DIR/$APP_NAME" 2>/dev/null | grep -q "com.apple.quarantine"; then
        log_info "Removing quarantine attribute..."
        xattr -d com.apple.quarantine "$INSTALL_DIR/$APP_NAME" 2>/dev/null || true
    fi
}

# Offer to launch the app
launch_app() {
    read -p "Would you like to launch Virtues now? [Y/n]: " result
    if [[ ! "$result" =~ ^[Nn]$ ]]; then
        log_info "Launching Virtues..."
        open "$INSTALL_DIR/$APP_NAME"
    fi
}

# Main installation flow
main() {
    echo ""
    echo "======================================"
    echo "       Virtues Mac Installer          "
    echo "======================================"
    echo ""

    check_macos
    check_existing
    install_app
    remove_quarantine

    echo ""
    log_success "Installation complete!"
    echo ""
    echo "To get started:"
    echo "  1. Launch Virtues from Applications"
    echo "  2. Click the menu bar icon"
    echo "  3. Select 'Pair with Virtues...'"
    echo "  4. Enter your pairing code from the web app"
    echo ""

    launch_app
}

main
