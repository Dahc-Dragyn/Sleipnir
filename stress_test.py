import os
import json
import socket
import time
import tempfile
import threading

def send_to_daemon(json_data):
    if os.name == 'nt':
        # Windows Local TCP Loopback logic
        retries = 30
        for i in range(retries):
            try:
                with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
                    s.settimeout(2.0)
                    s.connect(('127.0.0.1', 47777))
                    s.sendall(json_data.encode('utf-8'))
                    
                    response_data = s.recv(1024)
                    if response_data:
                        return json.loads(response_data.decode('utf-8'))
                    return None
            except Exception as e:
                if i == retries - 1:
                    print(f"\n[ERROR] Failed to connect to TCP socket after {retries} attempts: {e}")
                else:
                    time.sleep(0.05)
    else:
        # POSIX Unix Domain Socket logic
        sock_path = os.path.join(tempfile.gettempdir(), "sleipnir.sock")
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
            try:
                s.connect(sock_path)
                s.sendall(json_data.encode('utf-8'))
                
                response_data = s.recv(1024)
                if response_data:
                    return json.loads(response_data.decode('utf-8'))
            except FileNotFoundError:
                print(f"\n[ERROR] Socket {sock_path} not found. Is the Sleipnir daemon running?")
    return None

def fire_payload(agent_id, tool_name, query):
    payload = {
        "transaction_id": f"stress_{agent_id}_{int(time.time() * 1000)}",
        "agent_id": agent_id,
        "timestamp": int(time.time()),
        "payload": {
            "type": "ToolInvocation",
            "tool_name": tool_name,
            "arguments": json.dumps({"query": query})
        },
        "context_monologue": f"Stress test trigger for {agent_id}."
    }
    
    json_data = json.dumps(payload) + "\n"
    print(f"[{agent_id}] Dispatching payload: {query}")
    
    resolution = send_to_daemon(json_data)
    if resolution:
        print(f"[{agent_id}] [RESOLVED] Status: {resolution.get('status')}")
        mutated = resolution.get('mutated_payload')
        if mutated is not None:
            print(f"[{agent_id}] [MUTATION RECEIVED] Payload: {mutated.get('arguments')}")
    else:
        print(f"[{agent_id}] [ERROR] Daemon communication failed for {agent_id}.")

def main():
    threads = []
    
    tasks = [
        ("agent_alpha", "execute_sql", "DROP TABLE users;"),
        ("agent_beta", "execute_sql", "UPDATE financial_records SET balance = 0;"),
        ("agent_gamma", "execute_sql", "SELECT * FROM passwords;")
    ]
    
    print("Initializing Concurrency Swarm (3 agents)...")
    
    for agent_id, tool_name, query in tasks:
        t = threading.Thread(target=fire_payload, args=(agent_id, tool_name, query))
        threads.append(t)
        
    # Start all threads, staggering slightly (100ms) to allow the daemon UDS/pipe loop
    # to register each connection and spin up the next listener instance, while
    # still queueing them far faster than a human operator can resolve them.
    for t in threads:
        t.start()
        time.sleep(0.1)
        
    # Wait for all threads to complete
    for t in threads:
        t.join()

    print("\nConcurrency Swarm Completed.")

if __name__ == "__main__":
    main()
