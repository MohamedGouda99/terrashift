# 9 IAM roles. 9 of 50 source resources.
#
# Note: these are EXPECTED to skip during aws→azurerm migration.
# Stage 1 has no `azurerm_role_definition` template registered, so the
# Mapper has no target type to map them to. Best-case emit ratio is
# 41/50 = 82%, consistent with gate criterion #1's 80% threshold.

resource "aws_iam_role" "role_1" {
  name = "role-1"

  assume_role_policy = jsonencode({
    Version   = "2012-10-17"
    Statement = []
  })
}

resource "aws_iam_role" "role_2" {
  name = "role-2"

  assume_role_policy = jsonencode({
    Version   = "2012-10-17"
    Statement = []
  })
}

resource "aws_iam_role" "role_3" {
  name = "role-3"

  assume_role_policy = jsonencode({
    Version   = "2012-10-17"
    Statement = []
  })
}

resource "aws_iam_role" "role_4" {
  name = "role-4"

  assume_role_policy = jsonencode({
    Version   = "2012-10-17"
    Statement = []
  })
}

resource "aws_iam_role" "role_5" {
  name = "role-5"

  assume_role_policy = jsonencode({
    Version   = "2012-10-17"
    Statement = []
  })
}

resource "aws_iam_role" "role_6" {
  name = "role-6"

  assume_role_policy = jsonencode({
    Version   = "2012-10-17"
    Statement = []
  })
}

resource "aws_iam_role" "role_7" {
  name = "role-7"

  assume_role_policy = jsonencode({
    Version   = "2012-10-17"
    Statement = []
  })
}

resource "aws_iam_role" "role_8" {
  name = "role-8"

  assume_role_policy = jsonencode({
    Version   = "2012-10-17"
    Statement = []
  })
}

resource "aws_iam_role" "role_9" {
  name = "role-9"

  assume_role_policy = jsonencode({
    Version   = "2012-10-17"
    Statement = []
  })
}
