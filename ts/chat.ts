// Passed as a global from the backend.
declare const BAND_ROOM_ID: string;

const USERNAME_CMD = '/name';
const DEFAULT_USERNAME = 'anonymous';

interface Message {
    body: string;
    user: string;
    ts: string;
    id: string;
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

    /**
     * Start receiving messages from the room.
     */
    public async connect() {
        // Load history.
        const res = await fetch(`/${this.room}/history`);
        const data = await res.json();
        for (const msg of data) {
            this.addMessage(msg, false);
        }

        // Listen for new events.
        const source = new EventSource(`/${this.room}/chat`);
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

    /**
     * Try to resume an old session for this room, returning the current
     * username if the existing session is still valid.
     */
    async resume_session(): Promise<string | null> {
        const old_sess = localStorage.getItem(`session:${this.room}`);
        if (!old_sess) {
            return null;
        }

        let res;
        try {
            res = await fetch(`/${this.room}/session`, {
                headers: { 'Session': old_sess },
            });
        } catch (e) {
            return null;
        }
        const sess_data = await res.json();

        this.session = old_sess;
        return sess_data.user;
    }

    /**
     * Establish a brand-new session with a given username.
     */
    async new_session(user: string) {
        const res = await fetch(`/${this.room}/session`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({user: user}),
        });
        const new_sess = await res.text();

        localStorage.setItem(`session:${this.room}`, new_sess);
        this.session = new_sess;
    }

    /**
     * Resume or start a session in this room, which allows sending messages.
     */
    public async open_session(user: string) {
        // Try reusing an old session, if any exists.
        const old_user = await this.resume_session();
        if (old_user) {
            console.log(`resumed session ${this.session} as ${old_user}`);
        } else {
            await this.new_session(user);
            console.log(`started session ${this.session} as ${user}`);
        }
    }

    /**
     * Send a message. There must be an open session.
     */
    public async send(msg: string) {
        await fetch(`/${this.room}/message`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
                'Session': this.session!,
            },
            body: msg,
        });
    }

    /**
     * Change the current user. There must be an open session.
     */
    public async setUser(user: string) {
        await fetch(`/${this.room}/session`, {
            method: 'PUT',
            headers: {
                'Content-Type': 'application/json',
                'Session': this.session!,
            },
            body: JSON.stringify({user}),
        });

        this.addMessage({
            body: `you are now known as ${user}`,
            system: true,
        }, true);
    }

    /**
     * Vote for a message. There must be an open session.
     */
    public async vote(msgId: string, vote: boolean) {
        await fetch(`/${this.room}/message/${msgId}/vote`, {
            method: 'POST',
            headers: { 'Session': this.session! },
            body: vote ? "1" : "0",
        });
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
            line.dataset['id'] = msg.id;

            const user = document.createElement("span");
            user.classList.add("user");
            user.textContent = `${msg.user}:`;
            line.appendChild(user);

            const vote = document.createElement("button");
            vote.classList.add("vote");
            vote.textContent = "★";
            vote.addEventListener('click', handleVote);
            line.appendChild(vote);
        }

        const body = document.createElement("span");
        body.classList.add("body");
        line.appendChild(body);
        body.textContent = msg.body;

        outEl.appendChild(line);
        outContainerEl.scrollTop = 0;
    }

    async function handleVote(event: Event) {
        const msg = (event.target as Element).parentElement!;
        const id = msg.dataset['id']!;
        console.log(`voting for ${id}`);
        await client.vote(id, true);
        msg.classList.add("voted");
    }

    const client = new Client(BAND_ROOM_ID, addMessage);
    const connect_fut = client.connect();
    const user = localStorage.getItem('username') || DEFAULT_USERNAME;
    const session_fut = client.open_session(user);
    await connect_fut;
    await session_fut;

    formEl.addEventListener('submit', async (event) => {
        event.preventDefault();
        const text = msgEl.value;

        if (text.startsWith(USERNAME_CMD)) {
            // Update username.
            const newname = text.split(' ')[1];
            await client.setUser(newname);
            localStorage.setItem('username', user);
        } else {
            // Fire and forget; no need to await.
            client.send(text);
        }

        formEl.reset();
    });
});
