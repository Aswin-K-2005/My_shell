import { useState } from 'react'



function App() {
    // state
    const [messages, setMessages] = useState([])
    const [input, setInput] = useState("")

    // function to add a message
    function sendMessage() {
        setMessages([...messages, {text: input, sender: "user"}])
        setInput("")  // clear input
    }

    // render
    return (
        <div>
            {/* render all messages */}
            {messages.map((msg, index) => (
                <div key={index}>{msg.text}</div>
            ))}

            {/* input */}
            <input 
                value={input}
                onChange={(e) => setInput(e.target.value)}
            />
            <button onClick={sendMessage}>Send</button>
        </div>
    )
}


export default App
