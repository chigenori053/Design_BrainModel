import { useState } from "react";
// import { invoke } from "@tauri-apps/api/tauri"; // Commented out for now

type Message = { role: "user" | "system"; content: string };
type SemanticUnit = { id: string; type: string; content: string };

function App() {
    const [input, setInput] = useState("");
    const [history, setHistory] = useState<Message[]>([]);
    const [units, setUnits] = useState<SemanticUnit[]>([]);
    const [simulationResult, setSimulationResult] = useState<string | null>(null);

    async function handleSend() {
        if (!input.trim()) return;

        // 1. Add User Message
        const userMsg: Message = { role: "user", content: input };
        setHistory((prev) => [...prev, userMsg]);

        // 2. Mock IPC Call to VM (In real app, we'd use invoke("send_input", { content: input }))
        // Simulating response delay
        setTimeout(() => {
            // Mock State Update from VM
            mockVmResponse(input);
        }, 500);

        setInput("");
    }

    function mockVmResponse(userInput: string) {
        // Mock Semantic Extraction
        if (userInput.toLowerCase().includes("database")) {
            setUnits(prev => [...prev, {
                id: Date.now().toString(),
                type: "concept",
                content: "Database"
            }]);
        }
        if (userInput.toLowerCase().includes("must")) {
            setUnits(prev => [...prev, {
                id: Date.now().toString() + "_cons",
                type: "constraint",
                content: userInput
            }]);
        }

        // Mock Simulation
        if (userInput.toLowerCase().includes("run")) {
            setSimulationResult("Simulation started... Result: Success (Mock)");
        }
    }

    return (
        <div className="container">
            <h1>Design BrainModel (Phase 0)</h1>

            <div className="layout">
                <div className="chat-panel">
                    <h2>Conversation</h2>
                    <div className="history">
                        {history.map((msg, i) => (
                            <div key={i} className={`msg ${msg.role}`}>
                                <strong>{msg.role}:</strong> {msg.content}
                            </div>
                        ))}
                    </div>
                    <div className="input-area">
                        <input
                            value={input}
                            onChange={(e) => setInput(e.target.value)}
                            onKeyDown={(e) => e.key === "Enter" && handleSend()}
                            placeholder="Type a requirement..."
                        />
                        <button onClick={handleSend}>Send</button>
                    </div>
                </div>

                <div className="visual-panel">
                    <h2>Semantic Units</h2>
                    <div className="cards">
                        {units.map((u) => (
                            <div key={u.id} className={`card ${u.type}`}>
                                <div className="type">{u.type}</div>
                                <div className="content">{u.content}</div>
                            </div>
                        ))}
                    </div>

                    {simulationResult && (
                        <div className="simulation-result">
                            <h3>Last Simulation</h3>
                            <p>{simulationResult}</p>
                        </div>
                    )}
                </div>
            </div>
        </div>
    );
}

export default App;
