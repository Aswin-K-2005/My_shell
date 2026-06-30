import socket

s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect("/tmp/aish_memory.sock")
s.sendall(b"SAVE_MEMORY|project=My_shell|entity=architecture|content=Switched to unix sockets__MSG_END__")
s.close()
