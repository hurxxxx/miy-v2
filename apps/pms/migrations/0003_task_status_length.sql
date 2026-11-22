-- Workflow definitions allow status slugs of up to 40 characters.
ALTER TABLE pms_tasks ALTER COLUMN status TYPE varchar(40);
