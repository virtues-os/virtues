-- Migration: Update lite model slot from GLM 4.7 FlashX to GLM 5
UPDATE app_assistant_profile
SET lite_model_id = 'zai/glm-5'
WHERE lite_model_id = 'zai/glm-4.7-flashx';
