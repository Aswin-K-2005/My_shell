# chunker.py

def chunk_c_file(filepath):
    chunks=[]
    current_chunk=[]
    brace_depth=0
    in_function=False
    with open(filepath) as f:
        for line in f:
            if '{' in line:
                brace_depth+=line.count('{')
                in_function=True
            if '}' in line:
                brace_depth-=line.count('}')
            
            current_chunk.append(line)

            if in_function and brace_depth==0:
                chunks.append(''.join(current_chunk))
                in_function=False
                current_chunk=[]
    return chunks


def chunk_python_file(filepath):
    chunks = []
    current_chunk = []
    in_block = False
    
    with open(filepath) as f:
        for line in f:
            # detect function or class start
            stripped = line.lstrip()
            if stripped.startswith('def ') or stripped.startswith('class '):
                if current_chunk:
                    chunks.append(''.join(current_chunk))
                current_chunk = [line]
                in_block = True
            elif in_block:
                # check if we're still inside (line has content or is empty)
                if line.strip() == '':
                    current_chunk.append(line)
                elif line[0] != ' ' and line[0] != '\t':
                    # back to base indentation — block ended
                    chunks.append(''.join(current_chunk))
                    current_chunk = [line]
                    in_block = False
                else:
                    current_chunk.append(line)
            else:
                current_chunk.append(line)
    
    if current_chunk:
        chunks.append(''.join(current_chunk))
    
    return chunks
def chunk_markdown(filepath):
    chunks = []
    current_chunk = []
    
    with open(filepath) as f:
        for line in f:
            # new header = new chunk
            if line.startswith('#') and current_chunk:
                chunks.append(''.join(current_chunk))
                current_chunk = [line]
            else:
                current_chunk.append(line)
    
    if current_chunk:
        chunks.append(''.join(current_chunk))
    
    return chunks

def chunk_file(filepath):
    # route to correct chunker based on extension
    ext = filepath.rsplit('.', 1)[-1]
    if ext in ['c', 'h']:
        return chunk_c_file(filepath)
    elif ext == 'py':
        return chunk_python_file(filepath)
    elif ext in ['md', 'txt']:
        return chunk_markdown(filepath)
    else:
        return chunk_lines(filepath)

def chunk_lines(filepath, size=50):
    chunks=[]
    with open(filepath) as f:
        lines=f.readlines()
    for i in range(0,len(lines),size):
        chunks.append(''.join(lines[i:i+size]))

    return chunks
