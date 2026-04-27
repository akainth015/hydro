import * as cdk from 'aws-cdk-lib/core';
import { Construct } from 'constructs';
import { CfnCluster } from "aws-cdk-lib/aws-msk";
import { Vpc } from 'aws-cdk-lib/aws-ec2';

export class InfrastructureStack extends cdk.Stack {
  constructor(scope: Construct, id: string, props?: cdk.StackProps) {
    super(scope, id, props);

    const hydroAppVpc = new Vpc(this, 'HydroVpc', {
    });

    new CfnCluster(this, 'KafkaCluster', {
      clusterName: `${this.stackName}-Hydro`,
      kafkaVersion: '3.9.x.kraft',
      brokerNodeGroupInfo: {
        clientSubnets: hydroAppVpc.privateSubnets.map(subnet => subnet.subnetId),
        instanceType: 'kafka.m7g.large',
      },
      numberOfBrokerNodes: 2,
    });
  }
}
