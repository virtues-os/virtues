-- Add thought_signature to chat_messages for Gemini reasoning support
ALTER TABLE chat_messages ADD COLUMN thought_signature TEXT;
