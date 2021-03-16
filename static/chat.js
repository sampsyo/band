window.addEventListener('DOMContentLoaded', (event) => {
    const outEl = document.getElementById("output");
    const formEl = document.getElementById("send");
    const msgEl = document.getElementById("sendMessage");

    const source = new EventSource("/chat");
    source.addEventListener('open', (event) => {
        console.log("open", event);
    });
    source.addEventListener('error', (event) => {
        console.log("error", event);
    });
    source.addEventListener('message', (event) => {
        console.log("message", event);
        const line = document.createElement("p");
        line.textContent = event.data;
        outEl.appendChild(line);
    });

    formEl.addEventListener('submit', (event) => {
        const body = msgEl.value;

        fetch('/send', {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify(body),
        });

        formEl.reset();
        event.preventDefault();
    });
});
