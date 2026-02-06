-- Add parts column to chat_messages for storing UI message parts (tool invocations, web searches, etc.)
ALTER TABLE chat_messages ADD COLUMN parts TEXT;
