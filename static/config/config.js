let config = {
    all: false,
    movies: false,
    series: false,
    tv: false
};

const buttons = {
    all: document.getElementById('btn-all'),
    movies: document.getElementById('btn-movies'),
    series: document.getElementById('btn-series'),
    tv: document.getElementById('btn-tv')
};

function updateUI() {
    buttons.all.classList.toggle('active', config.all);
    buttons.movies.classList.toggle('active', config.movies);
    buttons.series.classList.toggle('active', config.series);
    buttons.tv.classList.toggle('active', config.tv);
}

function applyAllLogic() {
    const allSelected = config.movies && config.series && config.tv;
    config.all = allSelected;
    updateUI();
}

function toggle(type) {
    if (type === 'all') {
        config.all = !config.all;
        config.movies = config.all;
        config.series = config.all;
        config.tv = config.all;
    } else {
        config[type] = !config[type];

        if (config.all && !config[type]) {
            config.all = false;
        }
    }

    applyAllLogic();
    hideStatus();
}

function hideStatus() {
    document.getElementById('status').style.display = 'none';
}

async function loadConfig() {
    try {
        const resp = await fetch('/api/config', { method: 'GET' });
        if (resp.ok) {
            const data = await resp.json();
            config = {
                all: data.config.all,
                movies: data.config.movies,
                series: data.config.series,
                tv: data.config.tv
            };
            applyAllLogic();
        }
    } catch (err) {
        console.error('Failed to load config:', err);
    }
}

// Send update to backend
async function handleUpdate() {
    const btn = document.getElementById('update-btn');
    const originalText = btn.textContent;

    btn.disabled = true;
    btn.textContent = 'Updating…';

    try {
        const resp = await fetch('/api/update', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(config)
        });

        if (resp.ok) {
            const data = await resp.json();
            const statusEl = document.getElementById('status');
            if(data.success){
		statusEl.innerHTML = '✅ <strong>Library purged successfully!</strong><br><br><small><i>Purging will continue at fixed intervals</i></small>';
		statusEl.style.display = 'block';
            } else {
		statusEl.innerHTML = '❌ <strong>Error, update didn\'t work</strong><br>try logging out and back in to get a new auth key';
		statusEl.style.display = 'block';  
            }
        } else {
            document.getElementById('status').style.display = 'none';
            throw new Error('Update failed');
        }
    } catch (err) {
        console.error(err);
    } finally {
        btn.disabled = false;
        btn.textContent = originalText;
    }
}

async function handleLogout() {
    try { await fetch('/api/logout', { method: 'POST' }); } catch (e) {}
    window.location.href = '/';
}

function attachListeners() {
    buttons.all.addEventListener('click', () => toggle('all'));
    buttons.movies.addEventListener('click', () => toggle('movies'));
    buttons.series.addEventListener('click', () => toggle('series'));
    buttons.tv.addEventListener('click', () => toggle('tv'));

    document.getElementById('update-btn').addEventListener('click', handleUpdate);
    document.getElementById('logout-btn').addEventListener('click', handleLogout);

    Object.values(buttons).forEach(btn => {
        btn.addEventListener('click', hideStatus);
    });
}

window.onload = async () => {
    await loadConfig();
    attachListeners();
};
