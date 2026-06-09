CC = gcc
CFLAGS = -Wall -Wextra
TARGET = aish
SRCS = new.c ai.c compress.c

$(TARGET): $(SRCS)
	$(CC) $(CFLAGS) $(SRCS) -o $(TARGET)

clean:
	rm -f $(TARGET)

install:
	mkdir -p ~/.config/aish
	cp aish_ai.py ~/.config/aish/
	cp semantic_cache.py ~/.config/aish/
	cp chat_server.py ~/.config/aish/
	cp stopwords.txt ~/.config/aish/
	$(CC) $(CFLAGS) $(SRCS) -o $(TARGET)
