// TheWatcher Dashboard - vanilla JS, no external dependencies

const POLL_INTERVAL = 5000; // 5 seconds
const GAUGE_CIRCUMFERENCE = 2 * Math.PI * 50; // r=50

// ---- Theme ----
function initTheme() {
    const saved = getCookie('thewatcher_theme');
    const theme = saved || 'system';
    applyTheme(theme);
}

function applyTheme(theme) {
    if (theme === 'system') {
        const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
        document.documentElement.setAttribute('data-theme', prefersDark ? 'dark' : 'light');
    } else {
        document.documentElement.setAttribute('data-theme', theme);
    }
    setCookie('thewatcher_theme', theme, 365);
}

function cycleTheme() {
    const current = getCookie('thewatcher_theme') || 'system';
    const next = { system: 'light', light: 'dark', dark: 'system' }[current];
    applyTheme(next);
}

document.getElementById('theme-toggle').addEventListener('click', cycleTheme);

// Listen for system theme changes
window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
    if (getCookie('thewatcher_theme') === 'system') {
        applyTheme('system');
    }
});

// ---- Cookie helpers ----
function setCookie(name, value, days) {
    const d = new Date();
    d.setTime(d.getTime() + days * 86400000);
    document.cookie = `${name}=${value};path=/;samesite=lax;expires=${d.toUTCString()}`;
}

function getCookie(name) {
    const match = document.cookie.match(new RegExp('(^| )' + name + '=([^;]+)'));
    return match ? match[2] : null;
}

// ---- Formatting ----
function fmtBytes(bytes) {
    if (bytes === null || bytes === undefined) return '--';
    if (bytes >= 1e12) return (bytes / 1e12).toFixed(1) + ' TB';
    if (bytes >= 1e9) return (bytes / 1e9).toFixed(1) + ' GB';
    if (bytes >= 1e6) return (bytes / 1e6).toFixed(1) + ' MB';
    if (bytes >= 1e3) return (bytes / 1e3).toFixed(1) + ' KB';
    return bytes + ' B';
}

function fmtRate(bytesPerSec) {
    if (bytesPerSec === null || bytesPerSec === undefined) return '--';
    const units = ['B/s', 'KB/s', 'MB/s', 'GB/s'];
    let v = bytesPerSec;
    let i = 0;
    while (v >= 1000 && i < units.length - 1) { v /= 1000; i++; }
    return v.toFixed(1) + ' ' + units[i];
}

function fmtUptime(secs) {
    if (!secs) return '--';
    const d = Math.floor(secs / 86400);
    const h = Math.floor((secs % 86400) / 3600);
    const m = Math.floor((secs % 3600) / 60);
    const parts = [];
    if (d > 0) parts.push(d + 'd');
    if (h > 0 || d > 0) parts.push(h + 'h');
    parts.push(m + 'm');
    return parts.join(' ');
}

function fmtTime(ts) {
    if (!ts) return '--';
    return new Date(ts).toLocaleTimeString();
}

// Map internal series names to human-friendly labels
const SERIES_LABELS = {
    'cpu_percent':      'CPU',
    'load_1':           'Load (1m)',
    'used_percent':     'Used %',
    'rx_bytes_per_sec': 'RX (↓)',
    'tx_bytes_per_sec': 'TX (↑)',
    'process_count':    'Processes',
    'tcp_inuse':        'TCP',
};
function friendlyName(name) {
    return SERIES_LABELS[name] || name;
}

// Auto-scale display unit for chart values (raw API values → display values)
function displayUnit(rawUnit) {
    if (rawUnit === 'bytes/sec') return { unit: 'KB/s', scale: 1 / 1024 };
    return { unit: rawUnit, scale: 1 };
}

// ---- Gauge ----
function updateGauge(id, percent) {
    const circle = document.getElementById(id);
    if (!circle) return;
    const pct = Math.min(100, Math.max(0, percent || 0));
    const offset = GAUGE_CIRCUMFERENCE * (1 - pct / 100);
    circle.setAttribute('stroke-dasharray', GAUGE_CIRCUMFERENCE);
    circle.setAttribute('stroke-dashoffset', offset);

    // Color
    if (pct > 90) circle.style.stroke = 'var(--danger)';
    else if (pct > 70) circle.style.stroke = 'var(--warning)';
    else circle.style.stroke = id.includes('cpu') ? 'var(--gauge-cpu)' : 'var(--gauge-mem)';
}

// ---- Fetch & Update ----
let chartDataCache = {};

async function fetchCurrent() {
    try {
        const resp = await fetch('/api/current');
        if (!resp.ok) return;
        const data = await resp.json();
        updateDashboard(data);
    } catch (e) {
        console.error('Failed to fetch current metrics:', e);
    }
}

async function fetchHistory(metric, range, extraParams) {
    const params = new URLSearchParams({ metric, range });
    if (extraParams) {
        Object.entries(extraParams).forEach(([k, v]) => { if (v) params.append(k, v); });
    }
    try {
        const resp = await fetch('/api/history?' + params);
        if (!resp.ok) return null;
        return await resp.json();
    } catch (e) {
        console.error('Failed to fetch history:', e);
        return null;
    }
}

function updateDashboard(data) {
    // Hostname
    const host = data.hostname || '--';
    document.getElementById('hostname').textContent = host;
    document.title = 'TheWatcher — ' + host;
    document.getElementById('collection-time').textContent = 'Last: ' + fmtTime(data.timestamp_ms);

    // CPU gauge
    const cpuPct = data.cpu?.percent;
    document.getElementById('cpu-gauge-value').textContent = cpuPct != null ? cpuPct.toFixed(1) + '%' : '--';
    updateGauge('cpu-gauge-fill', cpuPct || 0);
    const loadStr = [data.cpu?.load_1, data.cpu?.load_5, data.cpu?.load_15]
        .map(v => v != null ? v.toFixed(2) : '--').join(' / ');
    document.getElementById('cpu-detail').textContent = 'Load: ' + loadStr;

    // Memory gauge
    const memPct = data.memory?.used_percent;
    document.getElementById('mem-gauge-value').textContent = memPct != null ? memPct.toFixed(1) + '%' : '--';
    updateGauge('mem-gauge-fill', memPct || 0);
    document.getElementById('mem-detail').textContent =
        fmtBytes(data.memory?.used_bytes) + ' / ' + fmtBytes(data.memory?.total_bytes);

    // System info
    document.getElementById('info-hostname').textContent = data.hostname || '--';
    document.getElementById('info-uptime').textContent = fmtUptime(data.uptime_seconds);
    document.getElementById('info-cpus').textContent = (data.cpu?.logical_cpus || '--') + ' logical';

    // Disks
    const diskContainer = document.getElementById('disk-cards');
    diskContainer.innerHTML = (data.disks || []).map(d => `
        <div class="card disk-card">
            <strong title="${escapeHtml(d.mount)}">${escapeHtml(truncatePath(d.mount, 32))}</strong>
            <div style="font-size:0.8rem;color:var(--text-secondary)">${escapeHtml(d.filesystem || '')}</div>
            <div class="bar-bg"><div class="bar-fill" style="width:${d.used_percent.toFixed(1)}%"></div></div>
            <div class="detail">${fmtBytes(d.used_bytes)} / ${fmtBytes(d.total_bytes)} (${d.used_percent.toFixed(1)}%)</div>
        </div>
    `).join('') || '<div class="card" style="color:var(--text-secondary)">No disks found</div>';

    // Networks — sort alphabetically by interface name so cards never jump around
    const netContainer = document.getElementById('network-cards');
    const sortedNets = (data.networks || []).slice().sort((a, b) => a.interface.localeCompare(b.interface));
    netContainer.innerHTML = sortedNets.map(n => `
        <div class="card net-card">
            <strong>${escapeHtml(n.interface)}</strong>
            <div style="font-size:0.8rem;color:var(--text-secondary);margin-top:4px">
                ↓ ${fmtRate(n.rx_bytes_per_sec)} &nbsp; ↑ ${fmtRate(n.tx_bytes_per_sec)}
            </div>
            <div style="font-size:0.75rem;color:var(--text-secondary);margin-top:2px">
                Total: ↓ ${fmtBytes(n.rx_bytes_total)} ↑ ${fmtBytes(n.tx_bytes_total)}
            </div>
        </div>
    `).join('') || '<div class="card" style="color:var(--text-secondary)">No interfaces found</div>';

    // Sockets + processes
    const sockContainer = document.getElementById('socket-cards');
    const tcp = data.sockets?.tcp_inuse;
    const udp = data.sockets?.udp_inuse;
    const total = data.sockets?.total_sockets;
    const procs = data.processes?.count;
    const sockAvailable = tcp != null;
    sockContainer.innerHTML = `
        <div class="card">
            <strong>Processes</strong>
            <div style="font-size:1.5rem;font-weight:700;margin-top:4px">${procs != null ? procs : '--'}</div>
        </div>
        <div class="card">
            <strong>TCP</strong>
            <div style="font-size:1.5rem;font-weight:700;margin-top:4px">${sockAvailable ? tcp : '--'}</div>
            <div style="font-size:0.75rem;color:var(--text-secondary)">in use</div>
        </div>
        <div class="card">
            <strong>UDP</strong>
            <div style="font-size:1.5rem;font-weight:700;margin-top:4px">${sockAvailable ? udp : '--'}</div>
            <div style="font-size:0.75rem;color:var(--text-secondary)">in use</div>
        </div>
        <div class="card">
            <strong>Total</strong>
            <div style="font-size:1.5rem;font-weight:700;margin-top:4px">${sockAvailable ? total : '--'}</div>
            <div style="font-size:0.75rem;color:var(--text-secondary)">sockets</div>
        </div>
    `;

    // Warnings
    const warnings = (data.collector_status || []).filter(s => s.status !== 'ok');
    const warnSection = document.getElementById('warnings-section');
    const warnList = document.getElementById('warnings-list');
    if (warnings.length > 0) {
        warnSection.style.display = 'block';
        warnList.innerHTML = warnings.map(w => `
            <div class="warning-item ${w.status}">
                <strong>${escapeHtml(w.component)}</strong>: ${escapeHtml(w.message || w.status)}
            </div>
        `).join('');
    } else {
        warnSection.style.display = 'none';
    }
}

function escapeHtml(str) {
    if (!str) return '';
    const div = document.createElement('div');
    div.textContent = str;
    return div.innerHTML;
}

function truncatePath(path, maxLen) {
    if (!path || path.length <= maxLen) return path;
    // Keep the first segment and last segment, truncate middle
    const parts = path.split('/');
    if (parts.length <= 3) {
        // Short path — just cut with ellipsis
        return path.substring(0, maxLen - 1) + '…';
    }
    // Keep first segment(s) + last segment, fill middle with …/
    const first = parts.slice(0, 2).join('/');
    const last = parts[parts.length - 1];
    // Build: /first/…/last, fit within maxLen
    const head = first + '/…/';
    const avail = maxLen - head.length;
    if (avail > 4) {
        return head + last.substring(last.length - avail);
    }
    return path.substring(0, maxLen - 1) + '…';
}

// ---- SVG Charts ----
function drawChart(containerId, data, seriesConfig) {
    const container = document.getElementById(containerId);
    if (!container) return;

    if (!data || !data.series || data.series.length === 0 || data.series.every(s => s.points.length === 0)) {
        container.innerHTML = '<div class="chart-empty">No data available</div>';
        return;
    }

    // Pick a human-friendly display unit (e.g. bytes/sec → KB/s)
    const rawUnit = data.series[0]?.unit || '';
    const du = displayUnit(rawUnit);
    const scale = du.scale;

    const margin = { top: 28, right: 40, bottom: 40, left: 60 };
    const width = container.clientWidth - margin.left - margin.right;
    const height = 250 - margin.top - margin.bottom;

    // Collect scaled values + timestamps
    let allValues = [];
    let allTimes = [];
    data.series.forEach(s => {
        s.points.forEach(p => {
            allTimes.push(p.timestamp_ms);
            if (p.value != null) allValues.push(p.value * scale);
            if (p.mean != null) allValues.push(p.mean * scale);
            if (p.max != null) allValues.push(p.max * scale);
            if (p.min != null) allValues.push(p.min * scale);
        });
    });

    if (allValues.length === 0) {
        container.innerHTML = '<div class="chart-empty">No data available</div>';
        return;
    }

    const timeMin = Math.min(...allTimes);
    const timeMax = Math.max(...allTimes);
    const valMin = Math.min(...allValues);
    const valMax = Math.max(...allValues);
    const valRange = (valMax - valMin) || 1;
    const valPad = valRange * 0.1;

    const xScale = t => margin.left + ((t - timeMin) / (timeMax - timeMin || 1)) * width;
    const yScale = v => margin.top + height - ((v - (valMin - valPad)) / (valRange + 2 * valPad)) * height;

    // Build SVG
    let svg = `<svg viewBox="0 0 ${width + margin.left + margin.right} ${height + margin.top + margin.bottom}">`;

    // Grid lines + Y-axis labels (display-unit values)
    for (let i = 0; i <= 4; i++) {
        const y = margin.top + (height * i / 4);
        const val = (valMin - valPad) + (valRange + 2 * valPad) * (1 - i / 4);
        svg += `<line x1="${margin.left}" y1="${y}" x2="${margin.left + width}" y2="${y}" stroke="var(--chart-grid)" stroke-width="0.5"/>`;
        svg += `<text x="${margin.left - 6}" y="${y + 4}" fill="var(--text-secondary)" font-size="10" text-anchor="end">${val.toFixed(1)}</text>`;
    }

    // X axis labels
    const timeSteps = 5;
    for (let i = 0; i <= timeSteps; i++) {
        const t = timeMin + (timeMax - timeMin) * i / timeSteps;
        const x = xScale(t);
        const label = new Date(t).toLocaleString(undefined, {
            month: 'short', day: 'numeric',
            hour: '2-digit', minute: '2-digit'
        });
        svg += `<text x="${x}" y="${margin.top + height + 20}" fill="var(--text-secondary)" font-size="10" text-anchor="middle">${label}</text>`;
    }

    // Series lines
    const colors = ['var(--chart-line-1)', 'var(--chart-line-2)', 'var(--chart-line-3)'];
    data.series.forEach((s, si) => {
        const color = colors[si % colors.length];
        const points = s.points.filter(p => p.value != null || p.mean != null);
        if (points.length < 2) return;

        let pathD = '';
        let areaD = '';
        points.forEach((p, i) => {
            const x = xScale(p.timestamp_ms);
            const raw = p.value != null ? p.value : p.mean;
            const y = yScale(raw * scale);
            pathD += (i === 0 ? 'M' : 'L') + `${x},${y} `;
            areaD += (i === 0 ? 'M' : 'L') + `${x},${y} `;
        });

        // Area fill
        const lastX = xScale(points[points.length - 1].timestamp_ms);
        const firstX = xScale(points[0].timestamp_ms);
        const bottomY = margin.top + height;
        svg += `<path d="${areaD} L${lastX},${bottomY} L${firstX},${bottomY} Z" fill="${color}" opacity="0.1"/>`;

        // Line
        svg += `<path d="${pathD}" fill="none" stroke="${color}" stroke-width="1.5"/>`;

        // Legend — stacked vertically, display unit
        const label = friendlyName(s.name) + ' (' + du.unit + ')';
        svg += `<text x="${margin.left}" y="${margin.top - 6 + si * 14}" fill="${color}" font-size="11" font-weight="500">${label}</text>`;
    });

    svg += '</svg>';

    // Add tooltip div
    const tooltipId = containerId + '-tooltip';
    svg += `<div id="${tooltipId}" class="chart-tooltip"></div>`;

    container.innerHTML = svg;

    // Hover overlay with scaled tooltip values
    const overlay = document.createElement('div');
    overlay.style.cssText = 'position:relative;margin-top:-250px;height:250px;';
    overlay.addEventListener('mousemove', (e) => {
        const rect = container.getBoundingClientRect();
        const x = e.clientX - rect.left;
        const t = timeMin + ((x - margin.left) / width) * (timeMax - timeMin);

        let nearest = null;
        let nearestDist = Infinity;
        data.series.forEach(s => {
            s.points.forEach(p => {
                const dist = Math.abs(p.timestamp_ms - t);
                if (dist < nearestDist) {
                    nearestDist = dist;
                    const raw = p.value != null ? p.value : (p.mean != null ? p.mean : 0);
                    nearest = { series: s.name, raw, ts: p.timestamp_ms };
                }
            });
        });

        const tooltip = document.getElementById(tooltipId);
        if (tooltip && nearest && nearestDist < (timeMax - timeMin) * 0.05) {
            tooltip.innerHTML = `<strong>${friendlyName(nearest.series)}</strong>: ${(nearest.raw * scale).toFixed(2)} ${du.unit}<br>${new Date(nearest.ts).toLocaleString()}`;
            tooltip.style.display = 'block';
            tooltip.style.left = (e.clientX - container.getBoundingClientRect().left + 10) + 'px';
            tooltip.style.top = (e.clientY - container.getBoundingClientRect().top - 30) + 'px';
        }
    });
    overlay.addEventListener('mouseleave', () => {
        const tooltip = document.getElementById(tooltipId);
        if (tooltip) tooltip.style.display = 'none';
    });
    container.appendChild(overlay);
}

// ---- History updates ----
async function updateAllCharts() {
    const cpuRange = document.getElementById('cpu-range').value;
    const memRange = document.getElementById('mem-range').value;
    const netRange = document.getElementById('net-range').value;
    const sockRange = document.getElementById('sock-range').value;

    const [cpuData, memData, netData, sockData] = await Promise.all([
        fetchHistory('cpu', cpuRange),
        fetchHistory('memory', memRange),
        fetchHistory('network', netRange),
        fetchHistory('sockets', sockRange),
    ]);

    if (cpuData) drawChart('cpu-chart', cpuData, []);
    if (memData) drawChart('mem-chart', memData, []);
    if (netData) drawChart('net-chart', netData, []);
    if (sockData) drawChart('sock-chart', sockData, []);
}

// ---- Init ----
async function fetchInfo() {
    try {
        const resp = await fetch('/api/info');
        if (!resp.ok) return;
        const data = await resp.json();
        document.getElementById('info-os').textContent = data.os || '--';
        document.getElementById('info-version').textContent = 'v' + (data.version || '--');
        document.getElementById('info-hostname').textContent = data.hostname || '--';
    } catch (e) {
        // info fetch best-effort, dashboard still works without it
    }
}

function init() {
    initTheme();

    // One-time: fetch static info for system card
    fetchInfo();

    // Restore saved range preferences, then wire change handlers
    ['cpu-range', 'mem-range', 'net-range', 'sock-range'].forEach(id => {
        const el = document.getElementById(id);
        // Restore cookie if set (overrides HTML default)
        const saved = getCookie('thewatcher_' + id);
        if (saved) el.value = saved;
        // Save cookie on change, then refresh charts
        el.addEventListener('change', () => {
            setCookie('thewatcher_' + id, el.value, 365);
            updateAllCharts();
        });
    });

    // Initial fetch
    fetchCurrent();
    updateAllCharts();

    // Poll for current metrics
    setInterval(fetchCurrent, POLL_INTERVAL);

    // Refresh charts every 60 seconds
    setInterval(updateAllCharts, 60000);

    // Handle resize
    window.addEventListener('resize', () => {
        updateAllCharts();
    });
}

document.addEventListener('DOMContentLoaded', init);
