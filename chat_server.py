import socket
import os
import requests
import json


sock_path="/tmp/aish_chat.sock"


if os.path.exists(sock_path):
    os.remove(sock_path)


server = socket.socket(socket.AF_UNIX,socket.SOCK_STREAM)
server.bind(sock_path)
server.listen(1)

print("Chat server Listening...")

while True:

    conn, _ =server.accept()

    data = conn.recv(4096).decode()

    response=requests.post("https://localhost:11434/api/generate",json={
        "model":"qwen3.5:9b",
        "prompt":data,
        "stream":True,
        "think":False,
        "options":{
            "temperature":0.7}
        },stream=True)


    for line in response.iter_lines():
        if line:
            chunk = json.loads(line)
            token = chunk.get("response","")
            if token:
                conn.send(token.encode())

    conn.send(b"__END__")
    conn.close()
