<script lang="ts">
    import { onMount } from "svelte";

    import { Page } from "$lib";
    import Icon from "$lib/components/Icon.svelte";
    import { spaceStore } from "$lib/stores/space.svelte";

    type ViewMode = 'tree' | 'graph';
    let viewMode = $state<ViewMode>('tree');

    // Sitemap data structure - neutral colors for graph only
    const sitemapData = {
        chats: {
            label: 'Chats',
            icon: 'ri:chat-3-line',
            graphColor: '#6b7280',
            routes: [
                { route: '/', label: 'New Chat', icon: 'ri:add-line' },
                { route: '/chat', label: 'All Chats', icon: 'ri:chat-history-line' },
                { route: '/chat/{id}', label: 'Chat', icon: 'ri:chat-1-line', isPattern: true },
            ]
        },
        pages: {
            label: 'Pages',
            icon: 'ri:file-list-3-line',
            graphColor: '#6b7280',
            routes: [
                { route: '/page', label: 'All Pages', icon: 'ri:file-list-3-line' },
                { route: '/page/{id}', label: 'Page', icon: 'ri:file-text-line', isPattern: true },
            ]
        },
        wiki: {
            label: 'Wiki',
            icon: 'ri:book-2-line',
            graphColor: '#6b7280',
            routes: [
                { route: '/wiki', label: 'Overview', icon: 'ri:compass-line' },
                { route: '/person', label: 'People', icon: 'ri:user-line' },
                { route: '/person/{id}', label: 'Person', icon: 'ri:user-line', isPattern: true },
                { route: '/place', label: 'Places', icon: 'ri:map-pin-line' },
                { route: '/place/{id}', label: 'Place', icon: 'ri:map-pin-line', isPattern: true },
                { route: '/org', label: 'Organizations', icon: 'ri:building-line' },
                { route: '/org/{id}', label: 'Organization', icon: 'ri:building-line', isPattern: true },
                { route: '/thing', label: 'Things', icon: 'ri:box-3-line' },
                { route: '/thing/{id}', label: 'Thing', icon: 'ri:box-3-line', isPattern: true },
                { route: '/day', label: 'Today', icon: 'ri:calendar-line' },
                { route: '/day/{date}', label: 'Day', icon: 'ri:calendar-line', isPattern: true },
                { route: '/year', label: 'This Year', icon: 'ri:calendar-2-line' },
                { route: '/year/{year}', label: 'Year', icon: 'ri:calendar-2-line', isPattern: true },
            ]
        },
        data: {
            label: 'Data',
            icon: 'ri:database-2-line',
            graphColor: '#6b7280',
            routes: [
                { route: '/source', label: 'Sources', icon: 'ri:plug-line' },
                { route: '/source/{id}', label: 'Source', icon: 'ri:database-2-line', isPattern: true },
                { route: '/drive', label: 'Drive', icon: 'ri:hard-drive-2-line' },
                { route: '/drive/{path}', label: 'File', icon: 'ri:file-line', isPattern: true },
            ]
        },
        system: {
            label: 'System',
            icon: 'ri:settings-3-line',
            graphColor: '#6b7280',
            routes: [
                { route: '/virtues/account', label: 'Account', icon: 'ri:user-settings-line' },
                { route: '/virtues/assistant', label: 'Assistant', icon: 'ri:robot-line' },
                { route: '/virtues/usage', label: 'Usage', icon: 'ri:bar-chart-line' },
                { route: '/virtues/jobs', label: 'Jobs', icon: 'ri:refresh-line' },
                { route: '/virtues/feedback', label: 'Feedback', icon: 'ri:feedback-line' },
                { route: '/virtues/sql', label: 'SQL Viewer', icon: 'ri:database-line' },
                { route: '/virtues/terminal', label: 'Terminal', icon: 'ri:terminal-box-line' },
                { route: '/virtues/sitemap', label: 'Sitemap', icon: 'ri:road-map-line' },
            ]
        },
        easter: {
            label: 'Easter Eggs',
            icon: 'ri:ghost-line',
            graphColor: '#6b7280',
            routes: [
                { route: '/life', label: 'Zen Garden', icon: 'ri:seedling-line' },
                { route: '/jump', label: 'Dog Jump', icon: 'ri:mickey-line' },
            ]
        }
    };

    function handleNavigate(route: string, label: string) {
        if (route.includes('{')) return;
        spaceStore.openTabFromRoute(route, { label });
    }

    // ============================================
    // Force-directed graph
    // ============================================
    let graphContainer: HTMLDivElement;
    let canvas: HTMLCanvasElement;
    let animationId: number;

    interface GraphNode {
        id: string;
        label: string;
        icon: string;
        color: string;
        x: number;
        y: number;
        vx: number;
        vy: number;
        radius: number;
        route?: string;
        isCategory?: boolean;
    }

    interface GraphEdge {
        source: GraphNode;
        target: GraphNode;
    }

    let nodes: GraphNode[] = [];
    let edges: GraphEdge[] = [];
    let hoveredNode: GraphNode | null = null;
    let isDragging = false;
    let dragNode: GraphNode | null = null;

    function initGraph() {
        if (!canvas) return;

        const width = graphContainer.clientWidth;
        const height = 400;
        canvas.width = width;
        canvas.height = height;
        const centerX = width / 2;
        const centerY = height / 2;

        nodes = [];
        edges = [];

        const centerNode: GraphNode = {
            id: 'root',
            label: 'Virtues',
            icon: 'ri:compass-3-line',
            color: '#6b7280',
            x: centerX,
            y: centerY,
            vx: 0,
            vy: 0,
            radius: 28,
            isCategory: true
        };
        nodes.push(centerNode);

        const categories = Object.entries(sitemapData);
        const angleStep = (2 * Math.PI) / categories.length;
        const categoryRadius = 130;

        categories.forEach(([key, category], i) => {
            const angle = angleStep * i - Math.PI / 2;
            const catNode: GraphNode = {
                id: key,
                label: category.label,
                icon: category.icon,
                color: category.graphColor,
                x: centerX + Math.cos(angle) * categoryRadius,
                y: centerY + Math.sin(angle) * categoryRadius,
                vx: 0,
                vy: 0,
                radius: 20,
                isCategory: true
            };
            nodes.push(catNode);
            edges.push({ source: centerNode, target: catNode });

            const routes = category.routes.filter(r => !r.isPattern);
            const routeAngleStep = (2 * Math.PI) / Math.max(routes.length, 1);
            const routeRadius = 60;

            routes.forEach((route, j) => {
                const routeAngle = angle + routeAngleStep * j - Math.PI;
                const routeNode: GraphNode = {
                    id: `${key}-${route.route}`,
                    label: route.label,
                    icon: route.icon,
                    color: category.graphColor,
                    x: catNode.x + Math.cos(routeAngle) * routeRadius + (Math.random() - 0.5) * 20,
                    y: catNode.y + Math.sin(routeAngle) * routeRadius + (Math.random() - 0.5) * 20,
                    vx: 0,
                    vy: 0,
                    radius: 12,
                    route: route.route
                };
                nodes.push(routeNode);
                edges.push({ source: catNode, target: routeNode });
            });
        });

        startSimulation();
    }

    function startSimulation() {
        const width = canvas.width;
        const height = canvas.height;
        const ctx = canvas.getContext('2d');
        if (!ctx) return;

        function simulate() {
            const alpha = 0.3;
            const repulsion = 600;
            const springLength = 70;
            const springStrength = 0.05;
            const damping = 0.85;
            const centerPull = 0.01;

            nodes.forEach(node => {
                if (!node.isCategory || node.id === 'root') {
                    node.vx += (width / 2 - node.x) * centerPull;
                    node.vy += (height / 2 - node.y) * centerPull;
                }
            });

            for (let i = 0; i < nodes.length; i++) {
                for (let j = i + 1; j < nodes.length; j++) {
                    const dx = nodes[j].x - nodes[i].x;
                    const dy = nodes[j].y - nodes[i].y;
                    const dist = Math.sqrt(dx * dx + dy * dy) || 1;
                    const force = repulsion / (dist * dist);
                    const fx = (dx / dist) * force;
                    const fy = (dy / dist) * force;
                    nodes[i].vx -= fx * alpha;
                    nodes[i].vy -= fy * alpha;
                    nodes[j].vx += fx * alpha;
                    nodes[j].vy += fy * alpha;
                }
            }

            edges.forEach(edge => {
                const dx = edge.target.x - edge.source.x;
                const dy = edge.target.y - edge.source.y;
                const dist = Math.sqrt(dx * dx + dy * dy) || 1;
                const force = (dist - springLength) * springStrength;
                const fx = (dx / dist) * force;
                const fy = (dy / dist) * force;
                edge.source.vx += fx * alpha;
                edge.source.vy += fy * alpha;
                edge.target.vx -= fx * alpha;
                edge.target.vy -= fy * alpha;
            });

            nodes.forEach(node => {
                if (node === dragNode) return;
                node.vx *= damping;
                node.vy *= damping;
                node.x += node.vx;
                node.y += node.vy;
                node.x = Math.max(node.radius, Math.min(width - node.radius, node.x));
                node.y = Math.max(node.radius, Math.min(height - node.radius, node.y));
            });

            ctx.clearRect(0, 0, width, height);

            // Edges
            ctx.strokeStyle = 'rgba(128, 128, 128, 0.15)';
            ctx.lineWidth = 1;
            edges.forEach(edge => {
                ctx.beginPath();
                ctx.moveTo(edge.source.x, edge.source.y);
                ctx.lineTo(edge.target.x, edge.target.y);
                ctx.stroke();
            });

            // Nodes
            nodes.forEach(node => {
                const isHovered = node === hoveredNode;

                ctx.beginPath();
                ctx.arc(node.x, node.y, node.radius, 0, 2 * Math.PI);
                ctx.fillStyle = isHovered ? 'rgba(128, 128, 128, 0.3)' : 'rgba(128, 128, 128, 0.08)';
                ctx.fill();
                ctx.strokeStyle = isHovered ? 'rgba(128, 128, 128, 0.6)' : 'rgba(128, 128, 128, 0.25)';
                ctx.lineWidth = isHovered ? 1.5 : 1;
                ctx.stroke();

                ctx.fillStyle = getComputedStyle(document.documentElement).getPropertyValue('--color-foreground') || '#888';
                ctx.font = node.isCategory ? '500 10px system-ui' : '10px system-ui';
                ctx.textAlign = 'center';
                ctx.textBaseline = 'middle';

                if (node.isCategory) {
                    ctx.fillText(node.label, node.x, node.y);
                } else if (isHovered) {
                    ctx.fillText(node.label, node.x, node.y + node.radius + 10);
                }
            });

            animationId = requestAnimationFrame(simulate);
        }

        simulate();
    }

    function handleCanvasMouseMove(e: MouseEvent) {
        if (!canvas) return;
        const rect = canvas.getBoundingClientRect();
        const x = e.clientX - rect.left;
        const y = e.clientY - rect.top;

        if (isDragging && dragNode) {
            dragNode.x = x;
            dragNode.y = y;
            dragNode.vx = 0;
            dragNode.vy = 0;
            return;
        }

        hoveredNode = null;
        for (const node of nodes) {
            const dx = x - node.x;
            const dy = y - node.y;
            if (dx * dx + dy * dy < node.radius * node.radius) {
                hoveredNode = node;
                canvas.style.cursor = node.route ? 'pointer' : 'grab';
                break;
            }
        }
        if (!hoveredNode) {
            canvas.style.cursor = 'default';
        }
    }

    function handleCanvasMouseDown() {
        if (hoveredNode) {
            isDragging = true;
            dragNode = hoveredNode;
            canvas.style.cursor = 'grabbing';
        }
    }

    function handleCanvasMouseUp() {
        if (isDragging && dragNode?.route && !didDrag) {
            handleNavigate(dragNode.route, dragNode.label);
        }
        isDragging = false;
        dragNode = null;
        didDrag = false;
    }

    let didDrag = false;
    $effect(() => {
        if (isDragging) {
            didDrag = true;
        }
    });

    function handleCanvasClick() {
        if (hoveredNode?.route && !didDrag) {
            handleNavigate(hoveredNode.route, hoveredNode.label);
        }
    }

    $effect(() => {
        if (viewMode === 'graph' && graphContainer) {
            setTimeout(() => initGraph(), 50);
        } else if (animationId) {
            cancelAnimationFrame(animationId);
        }
    });

    onMount(() => {
        return () => {
            if (animationId) cancelAnimationFrame(animationId);
        };
    });
</script>

<Page>
    <div class="sitemap-page">
        <header class="page-header">
            <div class="header-content">
                <h1>Sitemap</h1>
                <p class="subtitle">URL structure and data model reference</p>
            </div>

            <div class="view-toggle">
                <button
                    class="toggle-btn"
                    class:active={viewMode === 'tree'}
                    onclick={() => viewMode = 'tree'}
                >
                    <Icon icon="ri:list-check-2" width="16" />
                    <span>Routes</span>
                </button>
                <button
                    class="toggle-btn"
                    class:active={viewMode === 'graph'}
                    onclick={() => viewMode = 'graph'}
                >
                    <Icon icon="ri:mind-map" width="16" />
                    <span>Graph</span>
                </button>
            </div>
        </header>

        <!-- Tree/Routes View -->
        {#if viewMode === 'tree'}
            <div class="routes-section">
                {#each Object.entries(sitemapData) as [_key, category]}
                    <div class="route-group">
                        <div class="group-header">
                            <Icon icon={category.icon} width="16" />
                            <span>{category.label}</span>
                        </div>
                        <div class="route-list">
                            {#each category.routes as route}
                                <button
                                    class="route-item"
                                    class:pattern={route.isPattern}
                                    onclick={() => handleNavigate(route.route, route.label)}
                                    disabled={route.isPattern}
                                >
                                    <Icon icon={route.icon} width="14" />
                                    <span class="route-label">{route.label}</span>
                                    <code class="route-path">{route.route}</code>
                                </button>
                            {/each}
                        </div>
                    </div>
                {/each}
            </div>
        {/if}

        <!-- Graph View -->
        {#if viewMode === 'graph'}
            <div class="graph-section" bind:this={graphContainer}>
                <canvas
                    bind:this={canvas}
                    onmousemove={handleCanvasMouseMove}
                    onmousedown={handleCanvasMouseDown}
                    onmouseup={handleCanvasMouseUp}
                    onmouseleave={handleCanvasMouseUp}
                    onclick={handleCanvasClick}
                ></canvas>
                <p class="graph-hint">Drag to rearrange. Click to navigate.</p>
            </div>
        {/if}

        <!-- Namespace Concept -->
        <section class="doc-section">
            <h2>Namespace Concept</h2>
            <p class="section-desc">Every URL starts with a namespace. The first path segment determines storage and routing.</p>
            <div class="diagram-box">
                <pre>/drive/photos/vacation.jpg          /person/person_abc123
 └─┬─┘ └────────┬────────┘           └──┬──┘└─────┬──────┘
   │            │                       │         │
namespace    path within             namespace  entity_id
(filesystem)                         (sqlite)

/lake/sources/gmail/2024/inbox       /virtues/sitemap
 └┬─┘ └──────────┬──────────┘         └──┬───┘ └──┬──┘
  │              │                        │       │
namespace     S3 key path             namespace  subpath
(s3-like)                             (app)</pre>
            </div>
            <p class="note">Namespaces are always ONE level deep. Everything after is a path within that namespace.</p>
        </section>

        <!-- Views: Manual vs Smart -->
        <section class="doc-section">
            <h2>Views: Manual vs Smart</h2>
            <p class="section-desc">Views replace folders. Two types for organizing entities:</p>
            <table>
                <thead>
                    <tr>
                        <th>Type</th>
                        <th>How It Works</th>
                        <th>Like...</th>
                    </tr>
                </thead>
                <tbody>
                    <tr>
                        <td><strong>Manual</strong></td>
                        <td>User explicitly adds/removes items</td>
                        <td>Playlist</td>
                    </tr>
                    <tr>
                        <td><strong>Smart</strong></td>
                        <td>Query runs, items auto-populate</td>
                        <td>Smart Playlist</td>
                    </tr>
                </tbody>
            </table>
            <div class="diagram-box">
                <pre>// Manual view: hand-picked items (URLs)
{`{`}"view_type": "manual", "items": ["/person/person_abc", "/page/page_xyz"]{`}`}

// Smart view: auto-populates from query
{`{`}"view_type": "smart", "namespace": "person", "filter": {`{`}"tag": "work"{`}`}, "limit": 20{`}`}</pre>
            </div>
        </section>

        <!-- Folder System: URL-Native + Shallow Nesting -->
        <section class="doc-section">
            <h2>Folder System: URL-Native + Shallow Nesting</h2>
            <p class="section-desc">
                Folders can contain anything. Items are stored as URLs - the system infers type from the path.
            </p>

            <h3>URL → Type Resolution</h3>
            <table>
                <thead>
                    <tr>
                        <th>URL Pattern</th>
                        <th>Type</th>
                        <th>Resolution</th>
                    </tr>
                </thead>
                <tbody>
                    <tr>
                        <td><code>/person/person_abc</code></td>
                        <td>Record</td>
                        <td>SQLite lookup</td>
                    </tr>
                    <tr>
                        <td><code>/page/page_xyz</code></td>
                        <td>Record</td>
                        <td>SQLite lookup</td>
                    </tr>
                    <tr>
                        <td><code>/wiki</code></td>
                        <td>Route</td>
                        <td>App sitemap</td>
                    </tr>
                    <tr>
                        <td><code>/drive/docs/notes.pdf</code></td>
                        <td>File</td>
                        <td>Filesystem</td>
                    </tr>
                    <tr>
                        <td><code>/view/view_abc</code></td>
                        <td>Folder</td>
                        <td>SQLite lookup</td>
                    </tr>
                    <tr>
                        <td><code>https://arxiv.org</code></td>
                        <td>Link</td>
                        <td>External</td>
                    </tr>
                </tbody>
            </table>

            <p class="note">No type prefixes needed. URLs are the universal identifier.</p>

            <h3>Example: Project Plato</h3>
            <div class="diagram-box">
                <pre>view_items table (Project Plato folder):
┌────────────────────────────────┬───────────┐
│ url                            │ sort_order│
├────────────────────────────────┼───────────┤
│ https://arxiv.org/paper123     │ 0         │
│ /page/page_xyz                 │ 1         │
│ /drive/docs/plato-notes.pdf    │ 2         │
│ /person/person_abc             │ 3         │
│ /view/view_plato_chats         │ 4         │  ← sub-folder
└────────────────────────────────┴───────────┘

Rendered:
├── arxiv.org/paper123       (external link)
├── Plato Overview           (/page/page_xyz)
├── plato-notes.pdf          (/drive/docs/plato-notes.pdf)
├── Socrates                 (/person/person_abc)
└── Plato Chats/ (SMART)     (/view/view_plato_chats)
    └── auto-populated...</pre>
            </div>

            <p class="note">
                <strong>Depth limit:</strong> Folders can have sub-folders (depth=1), but sub-folders cannot have sub-sub-folders.
                Smart folders CAN be nested inside manual folders.
            </p>
        </section>

        <!-- Data Model -->
        <section class="doc-section">
            <h2>Data Model</h2>
            <p class="section-desc">Flat entities + connections + views. No folders.</p>
            <div class="diagram-box">
                <pre>┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│   ENTITIES (flat)              CONNECTIONS (graph)              │
│   ┌──────────────┐            ┌─────────────────────┐          │
│   │ person_abc   │───────────▶│ person_abc KNOWS    │          │
│   │ person_def   │            │ person_def          │          │
│   │ page_xyz     │◀───────────│ page_xyz MENTIONS   │          │
│   │ session_123  │            │ person_abc          │          │
│   │ place_ghi    │            └─────────────────────┘          │
│   └──────────────┘                                             │
│                                                                 │
│   VIEWS (saved queries OR manual lists)                        │
│   ┌──────────────────────────────────────────────────┐         │
│   │ 🔍 "Recent People" = smart query                 │         │
│   │ 📋 "Project Team" = manual list of entities      │         │
│   └──────────────────────────────────────────────────┘         │
│                                                                 │
│   SPACES (collections of URLs organized into folders)          │
│   ┌──────────────────────────────────────────────────┐         │
│   │ "Virtues" = system space (smart views only)      │         │
│   │ "Home" = user's default space                    │         │
│   │ "Work" = user-created space with URL refs        │         │
│   └──────────────────────────────────────────────────┘         │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘</pre>
            </div>
        </section>

        <!-- Design Principles -->
        <section class="doc-section">
            <h2>Design Principles</h2>
            <ul class="principles">
                <li><strong>Spaces are URL collections</strong> — Spaces are just collections of URLs organized into folders. Nothing more.</li>
                <li><strong>Flat namespaces</strong> — First segment only. No nesting like <code>/data/sources</code></li>
                <li><strong>Single app namespace</strong> — All system pages under <code>/virtues/*</code></li>
                <li><strong>Flat entities</strong> — No folders. Entities exist independently in their tables</li>
                <li><strong>Graph connections</strong> — Relationships via connections table, not hierarchy</li>
                <li><strong>Views not folders</strong> — Organization via manual lists or smart queries</li>
                <li><strong>Entity ID preserved</strong> — URL contains full DB entity ID exactly</li>
                <li><strong>DB is source of truth</strong> — All namespaces registered in <code>namespaces</code> table</li>
                <li><strong>AI-friendly</strong> — Flat queries, no tree traversal needed</li>
            </ul>
        </section>

        <!-- Storage Backends -->
        <section class="doc-section">
            <h2>Storage Backends</h2>
            <table>
                <thead>
                    <tr>
                        <th>Backend</th>
                        <th>Namespaces</th>
                        <th>Description</th>
                    </tr>
                </thead>
                <tbody>
                    <tr>
                        <td><code>sqlite</code></td>
                        <td>person, place, org, thing, day, year, page, session, source</td>
                        <td>Entity namespaces with full-text search</td>
                    </tr>
                    <tr>
                        <td><code>filesystem</code></td>
                        <td>/drive</td>
                        <td>Personal files on mounted storage</td>
                    </tr>
                    <tr>
                        <td><code>s3</code></td>
                        <td>/lake</td>
                        <td>Raw data lake for source ingestion</td>
                    </tr>
                    <tr>
                        <td><code>none</code></td>
                        <td>/virtues</td>
                        <td>App system pages (no data storage)</td>
                    </tr>
                </tbody>
            </table>
        </section>

        <!-- ID/URL Conversion -->
        <section class="doc-section">
            <h2>ID ↔ URL Conversion</h2>
            <p class="section-desc">Trivial bidirectional mapping for entities:</p>
            <div class="diagram-box">
                <pre>// Entity ID → URL
function getUrlFromEntityId(entityId: string): string {`{`}
    const namespace = entityId.split('_')[0];
    return `/${`$`}{`{`}namespace{`}`}/${`$`}{`{`}entityId{`}`}`;
{`}`}
// "person_abc123" → "/person/person_abc123"

// URL → Entity ID
function getEntityIdFromUrl(url: string): string {`{`}
    const segments = url.split('/');
    return segments[2];
{`}`}
// "/person/person_abc123" → "person_abc123"</pre>
            </div>
        </section>
    </div>
</Page>

<style>
    .sitemap-page {
        max-width: 900px;
        margin: 0 auto;
        padding: 2rem;
    }

    .page-header {
        display: flex;
        justify-content: space-between;
        align-items: flex-start;
        margin-bottom: 2rem;
        gap: 1rem;
        flex-wrap: wrap;
    }

    .header-content h1 {
        font-size: 1.5rem;
        font-weight: 600;
        margin: 0 0 0.25rem 0;
        color: var(--color-foreground);
    }

    .subtitle {
        color: var(--color-foreground-muted);
        margin: 0;
        font-size: 0.875rem;
    }

    /* View Toggle */
    .view-toggle {
        display: flex;
        background: var(--color-surface-elevated);
        border-radius: 6px;
        padding: 3px;
        gap: 2px;
    }

    .toggle-btn {
        display: flex;
        align-items: center;
        gap: 6px;
        padding: 6px 12px;
        border: none;
        border-radius: 4px;
        background: transparent;
        color: var(--color-foreground-muted);
        font-size: 0.8125rem;
        font-weight: 500;
        cursor: pointer;
        transition: all 0.15s ease;
    }

    .toggle-btn:hover {
        color: var(--color-foreground);
    }

    .toggle-btn.active {
        background: var(--color-background);
        color: var(--color-foreground);
    }

    /* Routes Section */
    .routes-section {
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
        gap: 1rem;
        margin-bottom: 2.5rem;
    }

    .route-group {
        background: var(--color-surface-elevated);
        border-radius: 8px;
        border: 1px solid var(--color-border);
        overflow: hidden;
    }

    .group-header {
        display: flex;
        align-items: center;
        gap: 8px;
        padding: 10px 14px;
        border-bottom: 1px solid var(--color-border);
        font-weight: 500;
        font-size: 0.8125rem;
        color: var(--color-foreground-muted);
        text-transform: uppercase;
        letter-spacing: 0.03em;
    }

    .group-header :global(svg) {
        opacity: 0.7;
    }

    .route-list {
        padding: 6px;
    }

    .route-item {
        display: flex;
        align-items: center;
        gap: 8px;
        width: 100%;
        padding: 6px 8px;
        border: none;
        border-radius: 4px;
        background: transparent;
        text-align: left;
        cursor: pointer;
        transition: background 0.1s ease;
        font-size: 0.8125rem;
        color: var(--color-foreground);
    }

    .route-item:hover:not(:disabled) {
        background: var(--color-surface-hover);
    }

    .route-item:disabled {
        cursor: default;
    }

    .route-item.pattern {
        opacity: 0.45;
    }

    .route-item :global(svg) {
        color: var(--color-foreground-muted);
        flex-shrink: 0;
        opacity: 0.7;
    }

    .route-label {
        flex: 1;
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .route-path {
        font-family: var(--font-mono);
        font-size: 0.6875rem;
        color: var(--color-foreground-subtle);
        background: var(--color-background);
        padding: 2px 5px;
        border-radius: 3px;
        flex-shrink: 0;
    }

    /* Graph Section */
    .graph-section {
        background: var(--color-surface-elevated);
        border-radius: 8px;
        border: 1px solid var(--color-border);
        padding: 1rem;
        margin-bottom: 2.5rem;
    }

    .graph-section canvas {
        display: block;
        width: 100%;
        border-radius: 6px;
    }

    .graph-hint {
        text-align: center;
        font-size: 0.75rem;
        color: var(--color-foreground-subtle);
        margin: 0.5rem 0 0 0;
    }

    /* Doc Sections */
    .doc-section {
        margin-bottom: 2rem;
    }

    .doc-section h2 {
        font-size: 1rem;
        font-weight: 600;
        color: var(--color-foreground);
        margin: 0 0 0.5rem 0;
        padding-bottom: 0.5rem;
        border-bottom: 1px solid var(--color-border);
    }

    .doc-section h3 {
        font-size: 0.875rem;
        font-weight: 600;
        color: var(--color-foreground);
        margin: 1rem 0 0.5rem 0;
    }

    .section-desc {
        color: var(--color-foreground-muted);
        font-size: 0.8125rem;
        margin: 0 0 0.75rem 0;
        line-height: 1.5;
    }

    .diagram-box {
        background: var(--color-surface-elevated);
        border: 1px solid var(--color-border);
        border-radius: 6px;
        padding: 1rem;
        overflow-x: auto;
    }

    .diagram-box pre {
        margin: 0;
        font-family: var(--font-mono);
        font-size: 0.75rem;
        line-height: 1.5;
        color: var(--color-foreground-muted);
    }

    .note {
        margin-top: 0.75rem;
        padding: 0.625rem 0.875rem;
        background: var(--color-surface-elevated);
        border-left: 2px solid var(--color-border);
        font-size: 0.8125rem;
        color: var(--color-foreground-muted);
    }

    table {
        width: 100%;
        border-collapse: collapse;
        font-size: 0.8125rem;
        margin-bottom: 0.75rem;
    }

    th, td {
        padding: 0.625rem 0.75rem;
        text-align: left;
        border-bottom: 1px solid var(--color-border);
    }

    th {
        font-weight: 500;
        color: var(--color-foreground-muted);
        background: var(--color-surface-elevated);
        font-size: 0.75rem;
        text-transform: uppercase;
        letter-spacing: 0.03em;
    }

    td {
        color: var(--color-foreground);
    }

    td code {
        background: var(--color-surface-elevated);
        padding: 0.125rem 0.375rem;
        border-radius: 3px;
        font-size: 0.75rem;
    }

    .principles {
        list-style: none;
        padding: 0;
        margin: 0;
    }

    .principles li {
        padding: 0.5rem 0;
        border-bottom: 1px solid var(--color-border);
        font-size: 0.8125rem;
        color: var(--color-foreground-muted);
    }

    .principles li:last-child {
        border-bottom: none;
    }

    .principles strong {
        color: var(--color-foreground);
    }

    .principles code {
        background: var(--color-surface-elevated);
        padding: 0.125rem 0.375rem;
        border-radius: 3px;
        font-size: 0.75rem;
    }

    @media (max-width: 640px) {
        .sitemap-page {
            padding: 1.5rem;
        }

        .page-header {
            flex-direction: column;
        }

        .routes-section {
            grid-template-columns: 1fr;
        }
    }
</style>
