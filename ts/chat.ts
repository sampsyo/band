// Passed as a global from the backend.
declare const BAND_ROOM_ID: string;

const USERNAME_CMD = '/name';
const DEFAULT_USERNAME = 'anonymous';

interface Message {
    body: string;
    user: string;
    ts: string;
}

interface SystemMessage {
    body: string;
    system: true;
}

class Client {
    session: string | undefined;

    constructor(
        public readonly room: string,
        public readonly addMessage: (msg: Message | SystemMessage,
                                     fresh: boolean) => void,
    ) { }

    public async start() {
        // Load history & start session.
        const history_fut = fetch(`/${BAND_ROOM_ID}/history`);
        const session_fut = fetch(`/${BAND_ROOM_ID}/session`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({user: this.getUser()}),
        });

        // Populate history.
        const history_data = await (await history_fut).json();
        for (const msg of history_data) {
            this.addMessage(msg, false);
        }

        // TODO Try reusing old session first.
        this.session = await (await session_fut).text();
        console.log(`started session ${this.session}`);

        // Listen for new events.
        const source = new EventSource(`/${BAND_ROOM_ID}/chat`);
        source.addEventListener('open', (event) => {
            console.log("open", event);
        });
        source.addEventListener('error', (event) => {
            console.log("error", event);
        });
        source.addEventListener('message', (event) => {
            console.log("message", event);
            this.addMessage(JSON.parse(event.data), true);
        });
    }

    public async send(msg: string) {
        await fetch(`/${this.room}/session/${this.session}/message`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: msg,
        });
    }

    getUser() {
        return localStorage.getItem('username') || DEFAULT_USERNAME;
    }

    public async setUser(user: string) {
        await fetch(`/${this.room}/session/${this.session}`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({user}),
        });
        localStorage.setItem('username', user);

        this.addMessage({
            body: `you are now known as ${user}`,
            system: true,
        }, true);
    }
}

window.addEventListener('DOMContentLoaded', async (event) => {
    const outEl = document.getElementById("messages")!;
    const outContainerEl = document.getElementById("output")!;
    const formEl = document.getElementById("send")! as HTMLFormElement;
    const msgEl = document.getElementById("sendMessage")! as HTMLInputElement;

    function addMessage(msg: Message | SystemMessage, fresh: boolean) {
        const line = document.createElement("p");
        if (fresh) {
            line.classList.add("fresh");
            setTimeout(() => line.classList.add("done"), 0);
        }

        if ("system" in msg) {
            line.classList.add("system");
        } else {
            const user = document.createElement("span");
            user.classList.add("user");
            line.appendChild(user);
            user.textContent = `${msg.user}:`;
        }

        const body = document.createElement("span");
        body.classList.add("body");
        line.appendChild(body);
        body.textContent = msg.body;

        outEl.appendChild(line);
        outContainerEl.scrollTop = 0;
    }

    const client = new Client(BAND_ROOM_ID, addMessage);
    await client.start();

    formEl.addEventListener('submit', async (event) => {
        event.preventDefault();
        const text = msgEl.value;

        if (text.startsWith(USERNAME_CMD)) {
            // Update username.
            const newname = text.split(' ')[1];
            await client.setUser(newname);
        } else {
            // Fire and forget; no need to await.
            client.send(text);
        }

        formEl.reset();
    });
});
