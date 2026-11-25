-- Reserve the scope used by local app data before the first dev installation.
-- No installation, release or permissions are created by this reservation.
ALTER TABLE apps ADD COLUMN initial_dev_installation_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE;
