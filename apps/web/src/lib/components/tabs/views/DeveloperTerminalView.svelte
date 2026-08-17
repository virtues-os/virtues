<script lang="ts">
    import type { Tab } from "$lib/tabs/types";
    import Icon from "$lib/components/Icon.svelte";
    import { onMount, onDestroy } from "svelte";
    import { browser } from "$app/environment";
    import { getWsUrl } from "$lib/config/backend";

    let { tab, active }: { tab: Tab; active: boolean } = $props();

    // Terminal state
    let terminalContainer: HTMLDivElement;
    let terminal: any = null;
    let fitAddon: any = null;
    let webSocket: WebSocket | null = null;
    let connectionStatus = $state<
        "disconnected" | "connecting" | "reconnecting" | "connected"
    >("disconnected");

    // Paste/drop upload state. Reported in the header, never written into the
    // terminal: a TUI owns that screen, and a status line printed under it would
    // be painted over — or worse, shift its layout.
    let uploading = $state(false);
    let uploadError = $state<string | null>(null);

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

    // WebSocket URL. getWsUrl() resolves same-origin on desktop (box-served) and
    // routes to the iroh loopback on mobile (bundled SPA at a tauri:// origin) —
    // the tauri:// scheme can't carry a WS upgrade, so location.host is wrong
    // there. Matches the Yjs path (see lib/config/backend.ts).
    const WS_URL = browser ? getWsUrl('/ws/terminal') : "";

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

            // Paste and drop of files. xterm's own paste handler only ever reads
            // `text/plain`, so image items fall through untouched and we can take
            // them in the capture phase before it runs.
            terminal.textarea?.addEventListener("paste", handlePaste, true);
            terminalContainer.addEventListener("dragover", handleDragOver);
            terminalContainer.addEventListener("drop", handleDrop);

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
            terminal?.textarea?.removeEventListener("paste", handlePaste, true);
            terminalContainer?.removeEventListener("dragover", handleDragOver);
            terminalContainer?.removeEventListener("drop", handleDrop);
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

    // -----------------------------------------------------------------------
    // Paste / drop -> a file on the box, whose path we type at the cursor.
    //
    // The clipboard is here in the browser; the shell is on the box. stdin has
    // never carried an image between the two — so the blob goes up as a file and
    // what lands in the terminal is its path, exactly as if the user had typed
    // it. `claude`, `vim`, `cat` all already know what to do with a path.
    // -----------------------------------------------------------------------

    /// Single-quote for the shell. Uploaded names are hash-based so they can't
    /// actually contain a quote, but a path that reaches a command line gets
    /// quoted properly regardless of what we believe about it.
    function shellQuote(path: string): string {
        return `'${path.replaceAll("'", `'\\''`)}'`;
    }

    async function uploadFile(file: File): Promise<string | null> {
        const resp = await fetch("/api/terminal/paste", {
            method: "POST",
            headers: {
                "Content-Type": file.type || "application/octet-stream",
            },
            body: file,
        });
        if (!resp.ok) {
            throw new Error(`${resp.status} ${await resp.text()}`);
        }
        const { path } = await resp.json();
        return path ?? null;
    }

    /// Upload each file and type its path into the terminal. Paths go in as
    /// ordinary input, so they land wherever the cursor is — including inside a
    /// running TUI's prompt.
    async function sendFiles(files: File[]) {
        if (!files.length || webSocket?.readyState !== WebSocket.OPEN) return;
        uploading = true;
        try {
            for (const file of files) {
                const path = await uploadFile(file);
                if (!path) continue;
                webSocket?.send(
                    JSON.stringify({
                        type: "input",
                        data: `${shellQuote(path)} `,
                    }),
                );
            }
        } catch (err) {
            console.error("[terminal] upload failed", err);
            uploadError = err instanceof Error ? err.message : "upload failed";
            setTimeout(() => (uploadError = null), 4000);
        } finally {
            uploading = false;
        }
    }

    function handlePaste(event: ClipboardEvent) {
        const files = Array.from(event.clipboardData?.items ?? [])
            .filter((item) => item.kind === "file")
            .map((item) => item.getAsFile())
            .filter((f): f is File => f !== null);
        if (!files.length) return; // plain text: let xterm paste it as usual

        // Stop xterm seeing it — otherwise it pastes the empty text/plain half of
        // the clipboard payload on top of our path.
        event.preventDefault();
        event.stopPropagation();
        void sendFiles(files);
    }

    function handleDragOver(event: DragEvent) {
        if (!event.dataTransfer?.types.includes("Files")) return;
        event.preventDefault();
        event.dataTransfer.dropEffect = "copy";
    }

    function handleDrop(event: DragEvent) {
        const files = Array.from(event.dataTransfer?.files ?? []);
        if (!files.length) return;
        event.preventDefault();
        void sendFiles(files);
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
            <!-- The wheel now scrolls tmux's history, which means the mouse
                 belongs to the terminal and click-drag no longer selects text.
                 Shift is the standard escape hatch, and nobody discovers it. -->
            <span class="hint">⇧ drag to select</span>
        </div>
        <div class="header-right">
            {#if uploadError}
                <span class="connection-badge error" title={uploadError}>
                    <Icon icon="ri:error-warning-line"/>
                    Upload failed
                </span>
            {:else if uploading}
                <span class="connection-badge">
                    <Icon icon="ri:loader-4-line" class="animate-spin"/>
                    Uploading
                </span>
            {/if}
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

    <!-- Terminal Container. The inner surface carries no padding of its own —
         see .terminal-surface below for why that matters. -->
    <div class="terminal-container">
        <div class="terminal-surface" bind:this={terminalContainer}></div>
    </div>
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

    .hint {
        font-size: 11px;
        color: var(--color-foreground-subtle);
    }

    .header-right {
        display: flex;
        align-items: center;
        gap: 8px;
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
        min-height: 0;
        display: flex;
        padding: 12px;
        overflow: hidden;
        background: var(--color-background);
    }

    /* The element xterm is opened into, and therefore the one FitAddon measures
       to decide how many rows and columns fit. It must carry no padding.
       FitAddon reads getComputedStyle(parent).height, which under the app's
       global border-box sizing is the *padding* box — so padding on this
       element is counted as room for cells that then have nowhere to go. At
       12px each way that is up to a row and a half too many, and the bottom
       row (a TUI's status line — tmux's, Claude's) is drawn half off the
       screen. Padding belongs on .terminal-container above, which FitAddon
       never sees. */
    .terminal-surface {
        flex: 1;
        min-height: 0;
        min-width: 0;
    }

    /* Ensure xterm fills the surface */
    .terminal-surface :global(.xterm) {
        height: 100%;
    }

    .terminal-surface :global(.xterm-viewport) {
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
