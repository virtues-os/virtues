<script lang="ts">
    import type { Tab } from "$lib/tabs/types";
    import Icon from "$lib/components/Icon.svelte";
    import { onMount, onDestroy } from "svelte";
    import { browser } from "$app/environment";

    let { tab, active }: { tab: Tab; active: boolean } = $props();

    // Terminal state
    let terminalContainer: HTMLDivElement;
    let terminal: any = null;
    let fitAddon: any = null;
    let webSocket: WebSocket | null = null;
    let connectionStatus = $state<
        "disconnected" | "connecting" | "reconnecting" | "connected"
    >("disconnected");

    // Reconnect state. The shell lives in tmux on the box, so a dropped socket
    // is a detach, not a death: reconnecting reattaches to the same session with
    // whatever was running still running. Worth retrying hard.
    let reconnectAttempts = 0;
    let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
    let hasAttached = false;
    let disposed = false;

    const RECONNECT_BASE_MS = 500;
    const RECONNECT_MAX_MS = 10_000;

    // Coalesce the ResizeObserver's per-frame firing during a window drag. Each
    // resize is an ioctl on the PTY plus a SIGWINCH, and a full-screen TUI
    // repaints its whole viewport on every one — unthrottled, dragging the
    // window edge is a redraw storm.
    const RESIZE_DEBOUNCE_MS = 100;

    // WebSocket URL — protocol-aware (matches Yjs pattern in document.ts)
    const wsProtocol = browser && location.protocol === 'https:' ? 'wss:' : 'ws:';
    const wsHost = browser ? location.host : 'localhost:8000';
    const WS_URL = browser ? `${wsProtocol}//${wsHost}/ws/terminal` : "";

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
            const { WebglAddon } = await import("@xterm/addon-webgl");
            const { Unicode11Addon } = await import("@xterm/addon-unicode11");

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

            // Unicode 11 widths. xterm defaults to the Unicode 6 tables, which
            // disagree with every modern terminal about how many cells an emoji
            // or a CJK glyph occupies. A TUI that draws a box, prints an emoji,
            // and draws the closing edge computes its own width from the modern
            // tables — so under Unicode 6 the box seams tear and the cursor
            // drifts a column per glyph.
            terminal.loadAddon(new Unicode11Addon());
            terminal.unicode.activeVersion = "11";

            // Open terminal in container
            terminal.open(terminalContainer);

            // WebGL renderer. The DOM renderer can't keep up with a full-screen
            // TUI repainting its viewport every frame — that's the laggy typing
            // and the tearing. Must load *after* open() (it needs the canvas),
            // and it's best-effort: no WebGL2 (a locked-down browser, a software
            // GL blocklist) or a lost context and we fall back to the DOM
            // renderer, which is slow but correct.
            try {
                const webglAddon = new WebglAddon();
                webglAddon.onContextLoss(() => webglAddon.dispose());
                terminal.loadAddon(webglAddon);
            } catch (err) {
                console.warn("[terminal] WebGL unavailable, using DOM renderer", err);
            }

            fitAddon.fit();

            // Welcome message — serif "Virtues" wordmark (matches the CLI banner)
            terminal.writeln("");
            terminal.writeln(
                "\x1b[90m              ,,                                            \x1b[0m",
            );
            terminal.writeln(
                "\x1b[90m`7MMF'   `7MF'db             mm                             \x1b[0m",
            );
            terminal.writeln(
                "\x1b[90m  `MA     ,V                 MM                             \x1b[0m",
            );
            terminal.writeln(
                "\x1b[90m   VM:   ,V `7MM  `7Mb,od8 mmMMmm `7MM  `7MM  .gP\"Ya  ,pP\"Ybd\x1b[0m",
            );
            terminal.writeln(
                "\x1b[90m    MM.  M'   MM    MM' \"'   MM     MM    MM ,M'   Yb 8I   `\"\x1b[0m",
            );
            terminal.writeln(
                "\x1b[90m    `MM A'    MM    MM       MM     MM    MM 8M\"\"\"\"\"\" `YMMMa.\x1b[0m",
            );
            terminal.writeln(
                "\x1b[90m     :MM;     MM    MM       MM     MM    MM YM.    , L.   I8\x1b[0m",
            );
            terminal.writeln(
                "\x1b[90m      VF    .JMML..JMML.     `Mbmo  `Mbod\"YML.`Mbmmd' M9mmmP'\x1b[0m",
            );
            terminal.writeln("");
            terminal.writeln("\x1b[1mTerminal\x1b[0m");
            terminal.writeln("");

            // Connect to WebSocket. Only now that fit() has run do we know the
            // real size to open the PTY at.
            connectWebSocket();

            // Handle terminal input. onData is already the encoded byte stream —
            // control chars, escape sequences, bracketed paste and mouse reports
            // all pass through untouched. Input while disconnected is dropped:
            // there is no local shell to echo it to, and the reconnect will
            // repaint the real one in a moment.
            terminal.onData((data: string) => {
                if (webSocket?.readyState === WebSocket.OPEN) {
                    webSocket.send(JSON.stringify({ type: "input", data }));
                }
            });

            // Handle window resize (debounced — see RESIZE_DEBOUNCE_MS)
            let resizeTimer: ReturnType<typeof setTimeout> | null = null;
            resizeObserver = new ResizeObserver(() => {
                if (resizeTimer) clearTimeout(resizeTimer);
                resizeTimer = setTimeout(() => {
                    // `disposed` guards a timer that survives teardown: the
                    // terminal object outlives dispose(), so a null check isn't
                    // enough — fit() on a disposed terminal throws.
                    if (disposed || !fitAddon || !terminal) return;
                    fitAddon.fit();
                    if (webSocket?.readyState === WebSocket.OPEN) {
                        webSocket.send(
                            JSON.stringify({
                                type: "resize",
                                cols: terminal.cols,
                                rows: terminal.rows,
                            }),
                        );
                    }
                }, RESIZE_DEBOUNCE_MS);
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
        // Stop the reconnect loop before tearing down, or it resurrects the
        // socket after the terminal it writes into is gone.
        disposed = true;
        if (reconnectTimer) clearTimeout(reconnectTimer);
        webSocket?.close();
        terminal?.dispose();
    });

    function connectWebSocket() {
        if (disposed || !terminal) return;
        connectionStatus = reconnectAttempts > 0 ? "reconnecting" : "connecting";

        // Open the PTY at the size we're actually showing. A TUI reads the
        // winsize once at startup, so a PTY born at 80x24 and resized a beat
        // later paints its first frame into the wrong box.
        const url = `${WS_URL}?cols=${terminal.cols}&rows=${terminal.rows}`;

        try {
            webSocket = new WebSocket(url);
            // Receive PTY output as raw bytes so xterm.js decodes UTF-8 itself
            // (a multibyte glyph can straddle two frames; decoding per-frame
            // would corrupt it).
            webSocket.binaryType = "arraybuffer";

            webSocket.onopen = () => {
                connectionStatus = "connected";
                reconnectAttempts = 0;
                // On a reattach the box's tmux repaints the full screen for us,
                // so clear first: what's on screen is a frozen snapshot from
                // before the drop, and anything left under the repaint shows
                // through as garbage.
                if (hasAttached) terminal?.reset();
                hasAttached = true;
            };

            webSocket.onmessage = (event) => {
                // Backend sends raw terminal output (ANSI escape sequences etc.)
                // as binary frames; control messages (e.g. the exit notice) as
                // text. xterm.js accepts both Uint8Array and string.
                if (event.data instanceof ArrayBuffer) {
                    terminal?.write(new Uint8Array(event.data));
                } else {
                    terminal?.write(event.data);
                }
            };

            // onerror always precedes onclose, so schedule the retry from
            // onclose alone and there's exactly one per drop.
            webSocket.onerror = () => {};
            webSocket.onclose = () => scheduleReconnect();
        } catch (err) {
            console.error("[terminal] WebSocket failed to open", err);
            scheduleReconnect();
        }
    }

    function scheduleReconnect() {
        if (disposed || reconnectTimer) return;
        connectionStatus = "reconnecting";

        const delay = Math.min(
            RECONNECT_BASE_MS * 2 ** reconnectAttempts,
            RECONNECT_MAX_MS,
        );
        reconnectAttempts += 1;

        // Say it once, on the first drop. The session is still alive on the box;
        // this is a lost connection, not a lost shell — and repeating the notice
        // on every backoff tick would scroll the screen we're about to restore.
        if (reconnectAttempts === 1) {
            terminal?.writeln(
                "\r\n\x1b[90m[disconnected — reconnecting…]\x1b[0m",
            );
        }

        reconnectTimer = setTimeout(() => {
            reconnectTimer = null;
            connectWebSocket();
        }, delay);
    }
</script>

<div class="terminal-wrapper">
    <!-- Header -->
    <div class="terminal-header">
        <div class="header-left">
            <Icon icon="ri:terminal-box-line"/>
            <span class="terminal-title">Terminal</span>
        </div>
        <div class="header-right">
            <span
                class="connection-badge"
                class:connected={connectionStatus === "connected"}
                class:error={connectionStatus === "disconnected"}
            >
                {#if connectionStatus === "connected"}
                    <Icon icon="ri:wifi-line"/>
                    Connected
                {:else if connectionStatus === "connecting"}
                    <Icon icon="ri:loader-4-line" class="animate-spin"/>
                    Connecting
                {:else if connectionStatus === "reconnecting"}
                    <Icon icon="ri:loader-4-line" class="animate-spin"/>
                    Reconnecting
                {:else}
                    <Icon icon="ri:wifi-off-line"/>
                    Disconnected
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
