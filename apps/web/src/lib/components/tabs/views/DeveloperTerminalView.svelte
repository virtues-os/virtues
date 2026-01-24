<script lang="ts">
    import type { Tab } from "$lib/tabs/types";
    import "iconify-icon";
    import { onMount, onDestroy } from "svelte";
    import { browser } from "$app/environment";

    let { tab, active }: { tab: Tab; active: boolean } = $props();

    // Terminal state
    let terminalContainer: HTMLDivElement;
    let terminal: any = null;
    let fitAddon: any = null;
    let webSocket: WebSocket | null = null;
    let connectionStatus = $state<
        "disconnected" | "connecting" | "connected" | "error"
    >("disconnected");
    let errorMessage = $state<string | null>(null);
    let inputBuffer = $state("");

    // WebSocket URL - in dev, connect to the backend
    // WebSocket URL - connect to backend core
    const WS_URL = browser
        ? `ws://${window.location.hostname}:8000/ws/terminal`
        : "";

    // Theme-aware colors (will be read from CSS vars at runtime)
    function getTerminalTheme() {
        if (!browser) return {};
        const style = getComputedStyle(document.documentElement);
        return {
            background:
                style.getPropertyValue("--background").trim() || "#0C0E13",
            foreground:
                style.getPropertyValue("--foreground").trim() || "#FAF9F5",
            cursor: style.getPropertyValue("--foreground").trim() || "#FAF9F5",
            cursorAccent:
                style.getPropertyValue("--background").trim() || "#0C0E13",
            selectionBackground:
                style.getPropertyValue("--highlight").trim() ||
                "rgba(255, 157, 82, 0.3)",
            black: style.getPropertyValue("--background").trim() || "#0C0E13",
            red: style.getPropertyValue("--error").trim() || "#ef4444",
            green: style.getPropertyValue("--success").trim() || "#22c55e",
            yellow: style.getPropertyValue("--warning").trim() || "#fbbf24",
            blue: style.getPropertyValue("--primary").trim() || "#60a5fa",
            magenta: "#c084fc",
            cyan: style.getPropertyValue("--primary").trim() || "#22d3ee",
            white: style.getPropertyValue("--foreground").trim() || "#FAF9F5",
            brightBlack:
                style.getPropertyValue("--foreground-subtle").trim() ||
                "#6b7280",
            brightRed: "#f87171",
            brightGreen: "#4ade80",
            brightYellow: "#fde047",
            brightBlue: "#93c5fd",
            brightMagenta: "#d8b4fe",
            brightCyan: "#67e8f9",
            brightWhite: "#FFFFFF",
        };
    }

    onMount(() => {
        if (!browser) return;

        let resizeObserver: ResizeObserver;
        let themeObserver: MutationObserver;

        const init = async () => {
            // Dynamically import xterm to avoid SSR issues
            const { Terminal } = await import("@xterm/xterm");
            const { FitAddon } = await import("@xterm/addon-fit");
            const { WebLinksAddon } = await import("@xterm/addon-web-links");

            // Import xterm CSS
            await import("@xterm/xterm/css/xterm.css");

            // Create terminal instance with theme-aware colors
            terminal = new Terminal({
                cursorBlink: true,
                cursorStyle: "bar",
                fontSize: 13,
                fontFamily: '"IBM Plex Mono", monospace',
                theme: getTerminalTheme(),
                allowProposedApi: true,
                scrollback: 10000,
            });

            // Add addons
            fitAddon = new FitAddon();
            terminal.loadAddon(fitAddon);
            terminal.loadAddon(new WebLinksAddon());

            // Open terminal in container
            terminal.open(terminalContainer);
            fitAddon.fit();

            // Welcome message with ASCII art
            terminal.writeln("");
            terminal.writeln(
                "\x1b[90m        _      _                   \x1b[0m",
            );
            terminal.writeln(
                "\x1b[90m /\\   /(_)_ __| |_ _   _  ___  ___ \x1b[0m",
            );
            terminal.writeln(
                "\x1b[90m \\ \\ / / | '__| __| | | |/ _ \\/ __|\x1b[0m",
            );
            terminal.writeln(
                "\x1b[90m  \\ V /| | |  | |_| |_| |  __/\\__ \\\x1b[0m",
            );
            terminal.writeln(
                "\x1b[90m   \\_/ |_|_|   \\__|\\__,_|\\___||___/\x1b[0m",
            );
            terminal.writeln("");
            terminal.writeln("\x1b[1mDeveloper Terminal\x1b[0m");
            terminal.writeln("");
            terminal.writeln(
                "\x1b[33m⚡ Coming Soon:\x1b[0m SSH + terminal access to your own server.",
            );
            terminal.writeln(
                "\x1b[90m   Full shell access for power users & developers.\x1b[0m",
            );
            terminal.writeln("");

            // Connect to WebSocket
            connectWebSocket();

            // Handle terminal input
            terminal.onData((data: string) => {
                if (webSocket && webSocket.readyState === WebSocket.OPEN) {
                    webSocket.send(JSON.stringify({ type: "input", data }));
                } else {
                    // Local echo mode when not connected
                    handleLocalInput(data);
                }
            });

            // Handle window resize
            resizeObserver = new ResizeObserver(() => {
                if (fitAddon) {
                    fitAddon.fit();
                    // Send resize event to backend
                    if (webSocket && webSocket.readyState === WebSocket.OPEN) {
                        webSocket.send(
                            JSON.stringify({
                                type: "resize",
                                cols: terminal.cols,
                                rows: terminal.rows,
                            }),
                        );
                    }
                }
            });
            resizeObserver.observe(terminalContainer);

            // Watch for theme changes (data-theme attribute on html element)
            themeObserver = new MutationObserver((mutations) => {
                for (const mutation of mutations) {
                    if (
                        mutation.type === "attributes" &&
                        mutation.attributeName === "data-theme"
                    ) {
                        // Update terminal theme
                        if (terminal) {
                            terminal.options.theme = getTerminalTheme();
                        }
                    }
                }
            });
            themeObserver.observe(document.documentElement, {
                attributes: true,
            });
        };

        init();

        return () => {
            if (resizeObserver) resizeObserver.disconnect();
            if (themeObserver) themeObserver.disconnect();
        };
    });

    onDestroy(() => {
        if (webSocket) {
            webSocket.close();
        }
        if (terminal) {
            terminal.dispose();
        }
    });

    function connectWebSocket() {
        connectionStatus = "connecting";

        try {
            webSocket = new WebSocket(WS_URL);

            webSocket.onopen = () => {
                connectionStatus = "connected";
                terminal?.writeln("\x1b[32m✓ Connected to backend\x1b[0m");
                terminal?.writeln("");
                showPrompt();
            };

            webSocket.onmessage = (event) => {
                try {
                    const msg = JSON.parse(event.data);
                    if (msg.type === "output") {
                        terminal?.write(msg.data);
                    } else if (msg.type === "error") {
                        terminal?.writeln(
                            `\x1b[31mError: ${msg.message}\x1b[0m`,
                        );
                    }
                } catch {
                    // Plain text output
                    terminal?.write(event.data);
                }
            };

            webSocket.onclose = () => {
                connectionStatus = "disconnected";
            };

            webSocket.onerror = () => {
                connectionStatus = "error";
                errorMessage = "Failed to connect";
                terminal?.writeln(
                    "\x1b[90mBackend not available. Running locally.\x1b[0m",
                );
                terminal?.writeln('\x1b[90mType "help" for commands.\x1b[0m');
                terminal?.writeln("");
                showPrompt();
            };
        } catch (err) {
            connectionStatus = "error";
            errorMessage = "WebSocket not supported";
            showPrompt();
        }
    }

    function showPrompt() {
        terminal?.write("\x1b[36mvirtues\x1b[0m $ ");
    }

    function handleLocalInput(data: string) {
        // Handle special keys
        if (data === "\r") {
            // Enter key
            terminal?.writeln("");
            processCommand(inputBuffer);
            inputBuffer = "";
            showPrompt();
        } else if (data === "\x7f") {
            // Backspace
            if (inputBuffer.length > 0) {
                inputBuffer = inputBuffer.slice(0, -1);
                terminal?.write("\b \b");
            }
        } else if (data === "\x03") {
            // Ctrl+C
            terminal?.writeln("^C");
            inputBuffer = "";
            showPrompt();
        } else if (data >= " " && data <= "~") {
            // Printable characters
            inputBuffer += data;
            terminal?.write(data);
        }
    }

    function processCommand(cmd: string) {
        const trimmed = cmd.trim();
        if (!trimmed) return;

        const [command, ...args] = trimmed.split(" ");

        switch (command.toLowerCase()) {
            case "help":
                terminal?.writeln("");
                terminal?.writeln("\x1b[1mCommands:\x1b[0m");
                terminal?.writeln("  help        Show this message");
                terminal?.writeln("  clear       Clear terminal");
                terminal?.writeln("  status      Connection status");
                terminal?.writeln("  reconnect   Retry connection");
                terminal?.writeln("  echo [msg]  Echo text");
                terminal?.writeln("");
                break;

            case "clear":
                terminal?.clear();
                break;

            case "status":
                const statusColor =
                    connectionStatus === "connected"
                        ? "32"
                        : connectionStatus === "connecting"
                          ? "33"
                          : "90";
                terminal?.writeln(
                    `Status: \x1b[${statusColor}m${connectionStatus}\x1b[0m`,
                );
                break;

            case "reconnect":
                terminal?.writeln("Reconnecting...");
                if (webSocket) {
                    webSocket.close();
                }
                connectWebSocket();
                break;

            case "echo":
                terminal?.writeln(args.join(" "));
                break;

            default:
                terminal?.writeln(`\x1b[90mUnknown: ${command}\x1b[0m`);
        }
    }
</script>

<div class="terminal-wrapper">
    <!-- Header -->
    <div class="terminal-header">
        <div class="header-left">
            <iconify-icon icon="ri:terminal-box-line"></iconify-icon>
            <span class="terminal-title">Terminal</span>
        </div>
        <div class="header-right">
            <span
                class="connection-badge"
                class:connected={connectionStatus === "connected"}
                class:error={connectionStatus === "error"}
            >
                {#if connectionStatus === "connected"}
                    <iconify-icon icon="ri:wifi-line"></iconify-icon>
                    Connected
                {:else if connectionStatus === "connecting"}
                    <iconify-icon icon="ri:loader-4-line" class="animate-spin"
                    ></iconify-icon>
                    Connecting
                {:else}
                    <iconify-icon icon="ri:computer-line"></iconify-icon>
                    Local
                {/if}
            </span>
        </div>
    </div>

    <!-- Terminal Container -->
    <div class="terminal-container" bind:this={terminalContainer}></div>
</div>

<style>
    .terminal-wrapper {
        display: flex;
        flex-direction: column;
        height: 100%;
        width: 100%;
        background: var(--color-background);
    }

    .terminal-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        height: 53px;
        padding: 0 16px;
        background: var(--color-surface);
        border-bottom: 1px solid var(--color-border);
        flex-shrink: 0;
    }

    .header-left {
        display: flex;
        align-items: center;
        gap: 8px;
        color: var(--color-foreground-muted);
        font-size: 14px;
    }

    .terminal-title {
        font-weight: 500;
    }

    .header-right {
        display: flex;
        align-items: center;
    }

    .connection-badge {
        display: flex;
        align-items: center;
        gap: 4px;
        font-size: 12px;
        padding: 4px 10px;
        border-radius: 6px;
        background: var(--color-surface-elevated);
        color: var(--color-foreground-muted);
    }

    .connection-badge.connected {
        color: var(--color-success);
    }

    .connection-badge.error {
        color: var(--color-foreground-subtle);
    }

    .terminal-container {
        flex: 1;
        padding: 12px;
        overflow: hidden;
        background: var(--color-background);
    }

    /* Ensure xterm fills container */
    .terminal-container :global(.xterm) {
        height: 100%;
    }

    .terminal-container :global(.xterm-viewport) {
        overflow-y: auto !important;
    }

    /* Custom scrollbar for terminal */
    .terminal-container :global(.xterm-viewport::-webkit-scrollbar) {
        width: 8px;
    }

    .terminal-container :global(.xterm-viewport::-webkit-scrollbar-track) {
        background: transparent;
    }

    .terminal-container :global(.xterm-viewport::-webkit-scrollbar-thumb) {
        background: var(--color-border);
        border-radius: 4px;
    }

    .terminal-container
        :global(.xterm-viewport::-webkit-scrollbar-thumb:hover) {
        background: var(--color-border-strong);
    }

    @keyframes spin {
        from {
            transform: rotate(0deg);
        }
        to {
            transform: rotate(360deg);
        }
    }

    .animate-spin {
        animation: spin 1s linear infinite;
    }
</style>
