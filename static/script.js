async function login() {
    const email = document.getElementById('email').value.trim();
    const password = document.getElementById('password').value;
    const auth_key = document.getElementById('auth_key').value.trim();
    const resultDiv = document.getElementById('result');
    if ((!auth_key && (!email || !password)) || (!!auth_key && !!email) || (!!auth_key && !!password)) {
        resultDiv.style.background = '#3c1a1a';
        resultDiv.style.color = '#f87171';
        resultDiv.innerHTML = 'Please provide auth key OR email and password';
        return;
    }

    resultDiv.style.background = '#422e1a';
    resultDiv.style.color = '#fbbf24';
    resultDiv.innerHTML = 'Logging in...';
    try {
	const payload = !!auth_key ? { auth_key } : { email, password };
        const resp = await fetch('/api/login', {
	    method: 'POST',
	    headers: { 'Content-Type': 'application/json' },
	    body: JSON.stringify(payload)
        });
        if (!resp.ok) {
            throw new Error(`HTTP ${resp.status}`);
        }
        const data = await resp.json();
        if (data.success) {
            resultDiv.style.background = '#1a3c1a';
            resultDiv.style.color = '#4ade80';
            resultDiv.innerHTML = `
            <strong>✅ Success!</strong><br>
            `;
            window.location.href = '/config';
        } else {
            resultDiv.style.background = '#3c1a1a';
            resultDiv.style.color = '#f87171';
            resultDiv.innerHTML = `<strong>❌ Error:</strong> ${data.error || 'Unknown error'}`;
        }
    } catch (err) {
        resultDiv.style.background = '#3c1a1a';
        resultDiv.style.color = '#f87171';
        resultDiv.innerHTML = `Failed to connect: ${err.message}`;
    }
}

function toggleLoginMode() {
    const emailInput = document.getElementById('email');
    const passwordInput = document.getElementById('password');
    const authKeyInput = document.getElementById('auth_key');
    const toggleBtn = document.getElementById('toggle-btn');

    if (emailInput.type === 'hidden') {
        emailInput.type = 'email';
        passwordInput.type = 'password';
        authKeyInput.type = 'hidden';
        toggleBtn.textContent = 'Use Auth Key';
    }
    else {
        emailInput.type = 'hidden';
        passwordInput.type = 'hidden';
        authKeyInput.type = 'text';
        toggleBtn.textContent = 'Use Email & Password';
    }
}
