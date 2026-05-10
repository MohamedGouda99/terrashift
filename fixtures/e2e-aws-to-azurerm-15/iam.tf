# 2 IAM roles. 2 of 15 source resources.
#
# Note: these two are EXPECTED to skip during aws→azurerm migration.
# Stage 1 has no `azurerm_role_definition` template registered, so the
# Mapper has no target type to map them to. The skip surfaces in the
# Migration summary's "Skipped (gaps)" section — that's by design and
# anchors the "≥12 of 15" gate threshold.

resource "aws_iam_role" "app_role" {
  name = "app-role"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Action = "sts:AssumeRole"
        Effect = "Allow"
        Principal = {
          Service = "ec2.amazonaws.com"
        }
      }
    ]
  })
}

resource "aws_iam_role" "db_role" {
  name = "db-role"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Action = "sts:AssumeRole"
        Effect = "Allow"
        Principal = {
          Service = "rds.amazonaws.com"
        }
      }
    ]
  })
}
