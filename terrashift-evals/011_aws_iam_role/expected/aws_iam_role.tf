resource "aws_iam_role" "migrator" {
  assume_role_policy = "{\"Version\":\"2012-10-17\",\"Statement\":[{\"Effect\":\"Allow\",\"Principal\":{\"Service\":\"ec2.amazonaws.com\"},\"Action\":\"sts:AssumeRole\"}]}"
  name = "terrashift-migrator"
  description = "Service account that runs Terrashift migrations"
}
