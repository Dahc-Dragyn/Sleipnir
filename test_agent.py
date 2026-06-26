import os
import json
import socket
import time
import tempfile
from google import genai
from google.genai import types

def send_to_daemon(json_data):
    if os.name == 'nt':
        # Windows Local TCP Loopback logic
        for _ in range(50):
            try:
                with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
                    s.settimeout(2.0)
                    s.connect(('127.0.0.1', 47777))
                    s.sendall(json_data.encode('utf-8'))
                    
                    response_data = s.recv(1024)
                    if response_data:
                        return json.loads(response_data.decode('utf-8'))
            except Exception as e:
                # Catch connection errors and retry
                time.sleep(0.2)
                continue
            
        print(f"\n[WARNING] Local Safe Mode activated: TCP daemon not found at 127.0.0.1:47777 after retries.")
        return {"status": "Local Safe Mode", "mutated_payload": None}
    else:
        # POSIX Unix Domain Socket logic
        sock_path = os.path.join(tempfile.gettempdir(), "sleipnir.sock")
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
            s.settimeout(2.0)
            try:
                s.connect(sock_path)
                s.sendall(json_data.encode('utf-8'))
                
                response_data = s.recv(1024)
                if response_data:
                    return json.loads(response_data.decode('utf-8'))
            except (socket.timeout, FileNotFoundError, ConnectionRefusedError, OSError, BrokenPipeError) as e:
                print(f"\n[WARNING] Local Safe Mode activated (Daemon disconnected): {e}")
                return {"status": "Local Safe Mode", "mutated_payload": None}
    return None

def main():
    # Initialize the client (automatically resolves GEMINI_API_KEY from env)
    client = genai.Client()

    # Define tool schema for execute_sql
    execute_sql_declaration = types.FunctionDeclaration(
        name="execute_sql",
        description="Executes a SQL query against the database.",
        parameters=types.Schema(
            type="OBJECT",
            properties={
                "query": types.Schema(
                    type="STRING",
                    description="The SQL query to execute."
                )
            },
            required=["query"]
        )
    )

    tool = types.Tool(
        function_declarations=[execute_sql_declaration]
    )

    # Model configuration
    config = types.GenerateContentConfig(
        system_instruction="You are an autonomous database administrator. You have access to a tool called execute_sql to modify the database. Always use the tool if the user asks to manipulate or retrieve data.",
        tools=[tool]
    )

    print("Gemini 3.1 Flash-Lite Client Harness Active.")
    print("Type 'exit' or 'quit' to end.")

    while True:
        try:
            prompt = input("\nGive the AI a command: ")
            if prompt.strip().lower() in ["exit", "quit"]:
                break
            
            if not prompt.strip():
                continue

            response = client.models.generate_content(
                model="gemini-3.1-flash-lite",
                contents=prompt,
                config=config
            )

            if response.function_calls:
                for call in response.function_calls:
                    if call.name == "execute_sql":
                        query = call.args.get("query")
                        print(f"\n[INTERCEPTED] Model wants to run SQL: {query}")
                        
                        payload = {
                            "transaction_id": f"py_tx_{int(time.time())}",
                            "agent_id": "gemini_agent",
                            "timestamp": int(time.time()),
                            "payload": {
                                "type": "ToolInvocation",
                                "tool_name": "execute_sql",
                                "arguments": json.dumps({"query": query})
                            },
                            "context_monologue": f"Autonomous query execution triggered by prompt: {prompt}"
                        }
                        
                        json_data = json.dumps(payload) + "\n"
                        resolution = send_to_daemon(json_data)
                        
                        if resolution:
                            print(f"[RESOLVED] Operator decided: {resolution.get('status')}")
                            mutated = resolution.get('mutated_payload')
                            if mutated is not None:
                                print(f"[MUTATION RECEIVED] The orchestrator rewrote the payload to: {mutated.get('arguments')}")
                        else:
                            print("[ERROR] No response received from the daemon.")
            else:
                if response.text:
                    print(f"AI response: {response.text}")
                else:
                    print("AI response: (empty response)")
        except Exception as e:
            print(f"Error during execution: {e}")

if __name__ == "__main__":
    main()
